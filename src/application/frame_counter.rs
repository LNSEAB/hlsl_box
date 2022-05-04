use super::*;

pub(super) struct FrameCounter {
    count: Cell<u64>,
    text_layout: RefCell<mltg::TextLayout>,
    frame_start_time: Cell<std::time::Instant>,
    ui_props: UiProperties,
}

impl FrameCounter {
    pub fn new(ui_props: &UiProperties) -> Result<Self, Error> {
        let text_layout = ui_props.factory.create_text_layout(
            "0",
            &ui_props.text_format,
            mltg::TextAlignment::Center,
            None,
        )?;
        Ok(Self {
            count: Cell::new(0),
            text_layout: RefCell::new(text_layout),
            frame_start_time: Cell::new(std::time::Instant::now()),
            ui_props: ui_props.clone(),
        })
    }

    pub fn reset(&self) {
        self.count.set(0);
        self.frame_start_time.set(std::time::Instant::now());
    }

    pub fn update(&self) -> Result<(), Error> {
        if (std::time::Instant::now() - self.frame_start_time.get()).as_millis() >= 1000 {
            let text_layout = self.ui_props.factory.create_text_layout(
                &self.count.get().to_string(),
                &self.ui_props.text_format,
                mltg::TextAlignment::Center,
                None,
            )?;
            *self.text_layout.borrow_mut() = text_layout;
            self.reset();
        } else {
            self.count.set(self.count.get() + 1);
        }
        Ok(())
    }

    pub fn draw(&self, cmd: &mltg::DrawCommand, pos: impl Into<mltg::Point>) {
        let margin = mltg::Size::new(5.0, 3.0);
        let text_layout = self.text_layout.borrow();
        let pos = pos.into();
        let size = text_layout.size();
        cmd.fill(
            &mltg::Rect::new(
                pos,
                [
                    size.width + margin.width * 2.0,
                    size.height + margin.height * 2.0,
                ],
            ),
            &self.ui_props.bg_color,
        );
        cmd.draw_text_layout(
            &text_layout,
            &self.ui_props.text_color,
            [pos.x + margin.width, pos.y + margin.height],
        );
    }
}