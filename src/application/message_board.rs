use super::*;

const X_MARGIN: f32 = 8.0;
const Y_MARGIN: f32 = 4.0;

trait Anim {
    fn draw(&self, cmd: &mltg::DrawCommand, size: wita::LogicalSize<f32>);
    fn update(&mut self, d: std::time::Duration) -> bool;
}

fn lerp<T>(a: T, b: T, p: T) -> T
where
    T: std::ops::Add<Output = T> + std::ops::Sub<Output = T> + std::ops::Mul<Output = T> + Copy,
{
    a + p * (b - a)
}

struct Slide {
    ui_props: UiProperties,
    text: mltg::TextLayout,
    start_position: mltg::Point,
    end_position: mltg::Point,
    t: f32,
    end_time: f32,
}

impl Anim for Slide {
    fn draw(&self, cmd: &mltg::DrawCommand, size: wita::LogicalSize<f32>) {
        let a = self.t / self.end_time;
        let position = mltg::point(
            lerp(
                size.width + self.start_position.x,
                size.width + self.end_position.x,
                a,
            ) - X_MARGIN * 2.0,
            self.end_position.y,
        );
        let text_size = self.text.size();
        let bg_size = mltg::size(
            text_size.width + X_MARGIN * 2.0,
            text_size.height + Y_MARGIN * 2.0,
        );
        cmd.fill(&mltg::rect(position, bg_size), &self.ui_props.bg_color);
        cmd.draw_text_layout(
            &self.text,
            &self.ui_props.text_color,
            (position.x + X_MARGIN, position.y + Y_MARGIN),
        );
    }

    fn update(&mut self, d: std::time::Duration) -> bool {
        self.t += d.as_secs_f32();
        (self.end_time - self.t) <= 0.0
    }
}

struct Rest {
    ui_props: UiProperties,
    text: mltg::TextLayout,
    position: mltg::Point,
    t: f32,
    end_time: f32,
}

impl Anim for Rest {
    fn draw(&self, cmd: &mltg::DrawCommand, size: wita::LogicalSize<f32>) {
        let text_size = self.text.size();
        let bg_size = mltg::size(
            text_size.width + X_MARGIN * 2.0,
            text_size.height + Y_MARGIN * 2.0,
        );
        let position = mltg::point(
            size.width + self.position.x - X_MARGIN * 2.0,
            self.position.y,
        );
        cmd.fill(&mltg::rect(position, bg_size), &self.ui_props.bg_color);
        cmd.draw_text_layout(
            &self.text,
            &self.ui_props.text_color,
            (position.x + X_MARGIN, position.y + Y_MARGIN),
        );
    }

    fn update(&mut self, d: std::time::Duration) -> bool {
        self.t += d.as_secs_f32();
        self.end_time - self.t <= 0.0
    }
}

struct Message {
    anim: Vec<Box<dyn Anim>>,
    p: usize,
    t: std::time::Instant,
}

impl Message {
    fn new(anim: Vec<Box<dyn Anim>>) -> Self {
        Self {
            anim,
            p: 0,
            t: std::time::Instant::now(),
        }
    }

    fn is_finished(&self) -> bool {
        self.p == self.anim.len()
    }

    fn draw(&mut self, cmd: &mltg::DrawCommand, size: wita::LogicalSize<f32>) {
        let a = &mut self.anim[self.p];
        a.draw(cmd, size);
        let t = std::time::Instant::now();
        let d = t - self.t;
        if a.update(d) {
            self.p += 1;
        }
        self.t = t;
    }
}

pub(super) struct MessageBoard {
    factory: mltg::Factory,
    ui_props: UiProperties,
    messages: RefCell<Vec<(Message, usize)>>,
    y_offset: f32,
}

impl MessageBoard {
    pub fn new(factory: &mltg::Factory, ui_props: &UiProperties, y_offset: f32) -> Self {
        Self {
            factory: factory.clone(),
            ui_props: ui_props.clone(),
            messages: RefCell::new(Vec::new()),
            y_offset,
        }
    }

    pub fn write(&mut self, text: impl AsRef<str>) -> anyhow::Result<()> {
        let text = self.factory.create_text_layout(
            text,
            &self.ui_props.text_format,
            mltg::TextAlignment::Leading,
            None,
        )?;
        let text_size = text.size();
        let mut messages = self.messages.borrow_mut();
        let mut indices = messages.iter().map(|(_, i)| *i).collect::<Vec<_>>();
        indices.sort_unstable();
        let mut iy = 0;
        for i in indices.iter() {
            if iy != *i {
                break;
            }
            iy += 1;
        }
        let y = (text_size.height + Y_MARGIN * 2.0) * iy as f32 + self.y_offset;
        let slide_in = Slide {
            ui_props: self.ui_props.clone(),
            text: text.clone(),
            start_position: (0.0, y).into(),
            end_position: (-text_size.width, y).into(),
            t: 0.0,
            end_time: 0.07,
        };
        let rest = Rest {
            ui_props: self.ui_props.clone(),
            text: text.clone(),
            position: slide_in.end_position,
            t: 0.0,
            end_time: 2.0,
        };
        let slide_out = Slide {
            ui_props: self.ui_props.clone(),
            text,
            start_position: slide_in.end_position,
            end_position: slide_in.start_position,
            t: 0.0,
            end_time: slide_in.end_time,
        };
        let anim = vec![
            Box::new(slide_in) as Box<dyn Anim>,
            Box::new(rest),
            Box::new(slide_out),
        ];
        messages.push((Message::new(anim), iy));
        Ok(())
    }

    pub fn draw(&self, cmd: &mltg::DrawCommand, size: wita::LogicalSize<f32>) {
        let messages = &mut self.messages.borrow_mut();
        for (msg, _) in messages.iter_mut() {
            msg.draw(cmd, size);
        }
        messages.retain(|msg| !msg.0.is_finished());
    }
}
