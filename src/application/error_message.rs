use super::*;
use gecl::Collision as _;
use regex::Regex;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TextColor {
    Text,
    Error,
    Warn,
    Info,
    UnderLine,
}

enum Layout {
    Text {
        layout: mltg::TextLayout,
        color: TextColor,
    },
    NewLine,
}

impl Layout {
    fn new_line_height(&self, h: f32) -> f32 {
        matches!(self, Self::NewLine).then(|| h).unwrap_or(0.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScrollBarState {
    None,
    Hover,
    Moving,
}

static RE: Lazy<Regex> = Lazy::new(|| Regex::new("(^.+:[0-9]+:[0-9]+: )(\\w+)(: )(.+)").unwrap());

pub(super) struct ErrorMessage {
    path: PathBuf,
    ui_props: UiProperties,
    text: Vec<String>,
    layouts: VecDeque<Vec<Layout>>,
    current_line: usize,
    scroll_bar_state: ScrollBarState,
    dy: f32,
    line_height: f32,
    hlsl_path: Option<PathBuf>,
}

impl ErrorMessage {
    pub fn new(
        path: PathBuf,
        e: &Error,
        ui_props: &UiProperties,
        view_size: wita::LogicalSize<f32>,
        hlsl_path: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let text = if &path == &*SETTINGS_PATH || &path == &*WINDOW_SETTING_PATH {
            format!("{}:\n{}", path.display(), e)
        } else {
            format!("{}", e)
        };
        let text = text.split('\n').map(|t| t.to_string()).collect::<Vec<_>>();
        let layouts = VecDeque::new();
        let mut this = Self {
            path,
            ui_props: ui_props.clone(),
            text,
            layouts,
            current_line: 0,
            scroll_bar_state: ScrollBarState::None,
            dy: 0.0,
            line_height: ui_props.line_height,
            hlsl_path,
        };
        let mut index = 0;
        let mut height = 0.0;
        while index < this.text.len() && height < view_size.height {
            let mut buffer = Vec::new();
            this.parse_text(&mut buffer, &this.text[index], view_size)?;
            height += buffer
                .iter()
                .fold(0.0, |h, l| h + l.new_line_height(this.line_height));
            this.layouts.push_back(buffer);
            index += 1;
        }
        Ok(this)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn hlsl_path(&self) -> Option<&PathBuf> {
        self.hlsl_path.as_ref()
    }

    pub fn offset(&mut self, view_size: wita::LogicalSize<f32>, d: i32) -> anyhow::Result<()> {
        if d == 0 {
            return Ok(());
        }
        let size = wita::LogicalSize::new(
            view_size.width - self.ui_props.scroll_bar.width,
            view_size.height,
        );
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
                self.parse_text(&mut buffer, &self.text[index as usize], size)?;
                self.layouts.push_front(buffer);
                index -= 1;
            }
            let mut height = self
                .layouts
                .iter()
                .flatten()
                .fold(0.0, |h, l| h + l.new_line_height(self.line_height));
            while height
                - self
                    .layouts
                    .back()
                    .unwrap()
                    .iter()
                    .fold(0.0, |h, l| h + l.new_line_height(self.line_height))
                > size.height
            {
                let back = self.layouts.pop_back().unwrap();
                height -= back
                    .iter()
                    .fold(0.0, |h, l| h + l.new_line_height(self.line_height));
            }
        } else {
            if d < self.layouts.len() as _ {
                self.layouts.drain(..d as usize);
            } else {
                self.layouts.clear();
            }
            let mut height = self
                .layouts
                .iter()
                .flatten()
                .fold(0.0, |h, l| h + l.new_line_height(self.line_height));
            let mut index = line as usize + self.layouts.len() - 1;
            while index < self.text.len() && height < size.height {
                let mut buffer = Vec::new();
                self.parse_text(&mut buffer, &self.text[index], size)?;
                height += buffer
                    .iter()
                    .fold(0.0, |h, l| h + l.new_line_height(self.line_height));
                self.layouts.push_back(buffer);
                index += 1;
            }
        }
        self.current_line = line;
        Ok(())
    }

    pub fn mouse_event(
        &mut self,
        mouse_pos: wita::LogicalPosition<f32>,
        button: Option<(wita::MouseButton, wita::KeyState)>,
        view_size: wita::LogicalSize<f32>,
    ) -> anyhow::Result<()> {
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
                let max_line = self.text.len() - 1;
                let line = ((mouse_pos.y - self.dy) * max_line as f32
                    / (view_size.height - thumb_size[1]))
                    .floor()
                    .clamp(0.0, max_line as f32) as i32;
                self.offset(view_size, line - self.current_line as i32)?;
                if let Some((wita::MouseButton::Left, wita::KeyState::Released)) = button {
                    if thumb_rc.is_crossing(&mouse_pos) {
                        self.scroll_bar_state = ScrollBarState::Hover;
                    } else {
                        self.scroll_bar_state = ScrollBarState::None;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn draw(&self, cmd: &mltg::DrawCommand, view_size: wita::LogicalSize<f32>) {
        cmd.fill(
            &mltg::Rect::new([0.0, 0.0], [view_size.width, view_size.height]),
            &self.ui_props.bg_color,
        );
        let mut y = 0.0;
        for line in &self.layouts {
            let mut x = 0.0;
            for l in line {
                match l {
                    Layout::Text {
                        layout: text,
                        color,
                    } => {
                        let color = match color {
                            TextColor::Text => &self.ui_props.text_color,
                            TextColor::Error => &self.ui_props.error_label_color,
                            TextColor::Warn => &self.ui_props.warn_label_color,
                            TextColor::Info => &self.ui_props.info_label_color,
                            TextColor::UnderLine => &self.ui_props.under_line_color,
                        };
                        cmd.draw_text_layout(text, color, [x, y]);
                        x += text.size().width;
                    }
                    Layout::NewLine => {
                        x = 0.0;
                        y += self.line_height;
                    }
                }
            }
        }
        self.draw_scroll_bar(cmd, view_size);
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
        cmd.fill(&mltg::Rect::new(thumb_origin, thumb_size), color);
    }

    pub fn recreate_text(&mut self, view_size: wita::LogicalSize<f32>) -> anyhow::Result<()> {
        let mut height = 0.0;
        let mut index = self.current_line as usize;
        self.layouts.clear();
        while index < self.text.len() && height < view_size.height {
            let mut buffer = Vec::new();
            self.parse_text(&mut buffer, &self.text[index], view_size)?;
            height += buffer
                .iter()
                .fold(0.0, |h, l| h + l.new_line_height(self.line_height));
            self.layouts.push_back(buffer);
            index += 1;
        }
        Ok(())
    }

    pub fn reset(
        &mut self,
        ui_props: &UiProperties,
        view_size: wita::LogicalSize<f32>,
    ) -> anyhow::Result<()> {
        self.ui_props = ui_props.clone();
        self.recreate_text(view_size)
    }

    fn parse_text(
        &self,
        buffer: &mut Vec<Layout>,
        text: &str,
        view_size: wita::LogicalSize<f32>,
    ) -> anyhow::Result<()> {
        if let Some(m) = RE.captures(text) {
            let x = self.create_text_layouts(
                buffer,
                m.get(1).unwrap().as_str(),
                view_size,
                TextColor::Text,
                0.0,
                false,
            )?;
            let t = m.get(2).unwrap().as_str();
            let color = if t.starts_with("error") {
                TextColor::Error
            } else if t.starts_with("warning") {
                TextColor::Warn
            } else if t.starts_with("info") {
                TextColor::Info
            } else {
                TextColor::Text
            };
            let x = self.create_text_layouts(buffer, t, view_size, color, x, true)?;
            let x = self.create_text_layouts(
                buffer,
                m.get(3).unwrap().as_str(),
                view_size,
                TextColor::Text,
                x,
                true,
            )?;
            self.create_text_layouts(
                buffer,
                m.get(4).unwrap().as_str(),
                view_size,
                TextColor::Text,
                x,
                true,
            )?;
        } else if text
            .chars()
            .all(|c| c.is_ascii_whitespace() || c == '~' || c == '^')
        {
            self.create_text_layouts(buffer, text, view_size, TextColor::UnderLine, 0.0, false)?;
        } else {
            self.create_text_layouts(buffer, text, view_size, TextColor::Text, 0.0, false)?;
        }
        buffer.push(Layout::NewLine);
        Ok(())
    }

    fn create_text_layouts(
        &self,
        v: &mut Vec<Layout>,
        text: &str,
        view_size: wita::LogicalSize<f32>,
        color: TextColor,
        x: f32,
        per_word: bool,
    ) -> Result<f32, Error> {
        let cs = text.chars().collect::<Vec<char>>();
        let mut x = x;
        let mut p = 0;
        let factory = &self.ui_props.factory;
        while p < text.len() {
            let layout = factory.create_text_layout(
                cs[p..].iter().collect::<String>(),
                &self.ui_props.text_format,
                mltg::TextAlignment::Leading,
                None,
            )?;
            let hit_test = layout.hit_test(mltg::Point::new(
                view_size.width - self.ui_props.scroll_bar.width - x,
                0.0,
            ));
            if !hit_test.inside {
                x += layout.size().width;
                v.push(Layout::Text { layout, color });
                break;
            }
            let mut q = p + hit_test.text_position;
            if per_word {
                if p == q {
                    v.push(Layout::NewLine);
                    x = 0.0;
                    continue;
                }
                while p < q && cs[q - 1].is_ascii() && cs[q - 1] != ' ' {
                    q -= 1;
                }
            }
            let s = cs[p..q].iter().collect::<String>();
            let layout = factory.create_text_layout(
                &s,
                &self.ui_props.text_format,
                mltg::TextAlignment::Leading,
                None,
            )?;
            v.push(Layout::Text { layout, color });
            v.push(Layout::NewLine);
            p = q;
            x = 0.0;
        }
        Ok(x)
    }
}
