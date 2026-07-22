#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderOptions {
    frame_width: u32,
    frame_height: u32,
    skin_id: String,
}

impl RenderOptions {
    pub fn new(frame_width: u32, frame_height: u32, skin_id: impl Into<String>) -> Self {
        Self {
            frame_width,
            frame_height,
            skin_id: skin_id.into(),
        }
    }
}
impl RenderOptions {
    pub fn frame_width(&self) -> u32 {
        self.frame_width
    }
}
impl RenderOptions {
    pub fn frame_height(&self) -> u32 {
        self.frame_height
    }
}
impl RenderOptions {
    pub fn skin_id(&self) -> &str {
        &self.skin_id
    }
}
