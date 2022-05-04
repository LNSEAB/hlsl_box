use super::*;

pub(super) struct ErrorMessage {
    path: PathBuf,
    window: wita::Window,
    ui_props: UiProperties,
    text: Vec<String>,
    layouts: VecDeque<Vec<mltg::TextLayout>>,
    current_line: u32,
}

impl ErrorMessage {
    pub fn new(
        path: PathBuf,
        window: wita::Window,
        e: &Error,
        ui_props: &UiProperties,
        size: mltg::Size,
    ) -> Result<Self, Error> {
        let text = format!("{}", e);
        let text = text.split('\n').map(|t| t.to_string()).collect::<Vec<_>>();
        let layouts = VecDeque::new();
        let mut this = Self {
            path,
            window,
            ui_props: ui_props.clone(),
            text,
            layouts,
            current_line: 0,
        };
        let mut index = 0;
        let mut height = 0.0;
        while index < this.text.len() && height < size.height {
            let mut buffer = Vec::new();
            this.create_text_layouts(&mut buffer, &this.text[index], size)?;
            height += buffer.iter().fold(0.0, |h, l| h + l.size().height);
            this.layouts.push_back(buffer);
            index += 1;
        }
        Ok(this)
    }

    pub fn offset(&mut self, size: mltg::Size, d: i32) -> Result<(), Error> {
        let mut line = self.current_line;
        if d < 0 {
            let d = d.abs() as u32;
            if line <= d {
                line = 0;
            } else {
                line -= d;
            }
        } else {
            line = (line + d as u32).min(self.text.len() as u32 - 1);
        }
        if self.current_line == line {
            return Ok(());
        }
        if self.current_line > line {
            let mut index = self.current_line as isize - 1;
            while index >= line as _ {
                let mut buffer = Vec::new();
                self.create_text_layouts(&mut buffer, &self.text[index as usize], size)?;
                self.layouts.push_front(buffer);
                index -= 1;
            }
            let mut height = self
                .layouts
                .iter()
                .flatten()
                .fold(0.0, |h, l| h + l.size().height);
            while height
                - self
                    .layouts
                    .back()
                    .unwrap()
                    .iter()
                    .fold(0.0, |h, l| h + l.size().height)
                > size.height
            {
                let back = self.layouts.pop_back().unwrap();
                height -= back.iter().fold(0.0, |h, l| h + l.size().height);
            }
        } else {
            let mut height = self
                .layouts
                .iter()
                .flatten()
                .fold(0.0, |h, l| h + l.size().height);
            let d = line - self.current_line;
            self.layouts.drain(..d as usize);
            let mut index = line as usize + self.layouts.len();
            while index < self.text.len() && height < size.height {
                let mut buffer = Vec::new();
                self.create_text_layouts(&mut buffer, &self.text[index], size)?;
                height += buffer.iter().fold(0.0, |h, l| h + l.size().height);
                self.layouts.push_back(buffer);
                index += 1;
            }
        }
        self.current_line = line;
        Ok(())
    }

    pub fn draw(&self, cmd: &mltg::DrawCommand) {
        let size = self
            .window
            .inner_size()
            .to_logical(self.window.dpi())
            .cast::<f32>();
        cmd.fill(
            &mltg::Rect::new([0.0, 0.0], [size.width, size.height]),
            &self.ui_props.bg_color,
        );
        let mut y = 0.0;
        for line in &self.layouts {
            for layout in line {
                cmd.draw_text_layout(layout, &self.ui_props.text_color, [0.0, y]);
                y += layout.size().height;
            }
        }
    }

    pub fn update(&mut self, size: mltg::Size) -> Result<(), Error> {
        let mut height = 0.0;
        let mut index = self.current_line as usize;
        self.layouts.clear();
        while index < self.text.len() && height < size.height {
            let mut buffer = Vec::new();
            self.create_text_layouts(&mut buffer, &self.text[index], size)?;
            height += buffer.iter().fold(0.0, |h, l| h + l.size().height);
            self.layouts.push_back(buffer);
            index += 1;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn create_text_layouts(
        &self,
        v: &mut Vec<mltg::TextLayout>,
        text: &str,
        size: mltg::Size,
    ) -> Result<(), Error> {
        let layout = self.ui_props.factory.create_text_layout(
            text,
            &self.ui_props.text_format,
            mltg::TextAlignment::Leading,
            None,
        )?;
        let test = layout.hit_test(mltg::point(size.width, 0.0));
        if !test.inside {
            v.push(layout);
            return Ok(());
        }
        let mut pos = test.text_position - 1;
        let cs = text.chars().collect::<Vec<char>>();
        let mut c = cs[pos];
        if c.is_ascii() {
            loop {
                if pos == 0 {
                    pos = test.text_position - 1;
                    break;
                }
                if !c.is_ascii() || c == ' ' {
                    break;
                }
                pos -= 1;
                c = cs[pos];
            }
        }
        let layout = self.ui_props.factory.create_text_layout(
            &cs.iter().take(pos + 1).collect::<String>(),
            &self.ui_props.text_format,
            mltg::TextAlignment::Leading,
            None,
        )?;
        v.push(layout);
        self.create_text_layouts(v, &cs.iter().skip(pos + 1).collect::<String>(), size)
    }
}

