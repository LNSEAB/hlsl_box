use super::*;

pub struct Messages {
    pub screen_shot: &'static str,
    pub record_video_start: &'static str,
    pub record_video_end: &'static str,
}

impl Messages {
    fn new(loc: Option<&str>) -> Self {
        match loc {
            Some("ja-JP") => Self {
                screen_shot: "スクリーンショットを撮影",
                record_video_start: "録画を開始",
                record_video_end: "録画を終了",
            },
            _ => Self {
                screen_shot: "take the screenshot",
                record_video_start: "start recoding",
                record_video_end: "end recoding",
            },
        }
    }
}

pub static MESSAGES: Lazy<Messages> =
    Lazy::new(|| Messages::new(LOCALE.as_ref().map(|l| l.as_str())));
