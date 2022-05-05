use super::*;
use gecl::Collision as _;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScrollBarState {
    None,
    Hover,
    Moving,
}

pub(super) struct ErrorMessage {
    path: PathBuf,
    window: wita::Window,
    ui_props: UiProperties,
    text: Vec<String>,
    layouts: VecDeque<Vec<mltg::TextLayout>>,
    current_line: usize,
    scroll_bar_state: ScrollBarState,
    dy: f32,
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
            scroll_bar_state: ScrollBarState::None,
            dy: 0.0,
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

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn offset(&mut self, size: wita::LogicalSize<f32>, d: i32) -> Result<(), Error> {
        if d == 0 {
            return Ok(());
        }
        let size = mltg::Size::new(size.width - self.ui_props.scroll_bar.width, size.height);
        let mut line = self.current_line;
        if d < 0 {
            let d = d.abs() as usize;
            if line <= d {
                line = 0;
            } else {
                line -= d;
            }
        } else {
            line = (line + d as usize).min(self.text.len() - 1);
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

    pub fn mouse_event(
        &mut self,
        view_size: wita::LogicalSize<f32>,
        mouse_pos: wita::LogicalPosition<f32>,
        button: Option<(wita::MouseButton, wita::KeyState)>,
    ) {
        let props = &self.ui_props.scroll_bar;
        let line_height = self.ui_props.line_height;
        let x = view_size.width - props.width;
        let a = self.text.len() as f32 + view_size.height / line_height - 1.0;
        let thumb_origin = [x, self.current_line as f32 * view_size.height / a];
        let thumb_size = [
            props.width,
            view_size.height * view_size.height / line_height / a,
        ];
        let mouse_pos = gecl::point(mouse_pos.x, mouse_pos.y);
        let thumb_rc = gecl::rect(thumb_origin, thumb_size);
        match self.scroll_bar_state {
            ScrollBarState::None => {
                if thumb_rc.is_crossing(&mouse_pos) {
                    if let Some((wita::MouseButton::Left, wita::KeyState::Pressed)) = button {
                        self.scroll_bar_state = ScrollBarState::Moving;
                        self.dy = mouse_pos.y - thumb_origin[1];
                    } else {
                        self.scroll_bar_state = ScrollBarState::Hover;
                    }
                }
            }
            ScrollBarState::Hover => {
                if thumb_rc.is_crossing(&mouse_pos) {
                    if let Some((wita::MouseButton::Left, wita::KeyState::Pressed)) = button {
                        self.scroll_bar_state = ScrollBarState::Moving;
                        self.dy = mouse_pos.y - thumb_origin[1];
                    }
                } else {
                    self.scroll_bar_state = ScrollBarState::None;
                }
            }
            ScrollBarState::Moving => {
                let line = ((mouse_pos.y - self.dy) / (self.text.len() - 1) as f32)
                    .floor()
                    .clamp(0.0, (self.text.len() - 1) as f32) as usize;
                self.offset(view_size, line as i32 - self.current_line as i32)
                    .unwrap();
                if let Some((wita::MouseButton::Left, wita::KeyState::Released)) = button {
                    if thumb_rc.is_crossing(&mouse_pos) {
                        self.scroll_bar_state = ScrollBarState::Hover;
                    } else {
                        self.scroll_bar_state = ScrollBarState::None;
                    }
                }
            }
        }
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
        self.draw_scroll_bar(cmd, size);
    }

    fn draw_scroll_bar(&self, cmd: &mltg::DrawCommand, view_size: wita::LogicalSize<f32>) {
        let props = &self.ui_props.scroll_bar;
        let line_height = self.ui_props.line_height;
        let bg_origin = [view_size.width - props.width, 0.0];
        let bg_size = [props.width, view_size.height];
        cmd.fill(&mltg::Rect::new(bg_origin, bg_size), &props.bg_color);
        let a = self.text.len() as f32 + view_size.height / line_height - 1.0;
        let thumb_origin = [
            bg_origin[0],
            self.current_line as f32 * view_size.height / a,
        ];
        let thumb_size = [
            props.width,
            view_size.height * view_size.height / line_height / a,
        ];
        let color = match self.scroll_bar_state {
            ScrollBarState::None => &props.thumb_color,
            ScrollBarState::Hover => &props.thumb_hover_color,
            ScrollBarState::Moving => &props.thumb_moving_color,
        };
        cmd.fill(&mltg::Rect::new(thumb_origin, thumb_size), &color);
    }

    pub fn recreate(&mut self, size: mltg::Size) -> Result<(), Error> {
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
