use crate::domain::{models::AvatarImage, AvatarError};

pub trait AvatarProcessor: Send + Sync + 'static {
    fn process(&self, input: &[u8], content_type: Option<&str>)
        -> Result<AvatarImage, AvatarError>;
}
