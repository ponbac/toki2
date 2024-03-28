use serde::Serialize;
use web_push::{
    ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushError, WebPushMessage,
    WebPushMessageBuilder, URL_SAFE,
};

#[derive(Debug, Serialize)]
pub struct PushNotification {
    title: String,
    body: String,
    icon: Option<String>,
}

impl From<&PushNotification> for Vec<u8> {
    fn from(notification: &PushNotification) -> Self {
        serde_json::to_vec(notification).expect("Could not serialize notification")
    }
}

impl PushNotification {
    pub fn new(title: &str, body: &str, icon: Option<&str>) -> Self {
        PushNotification {
            title: title.to_string(),
            body: body.to_string(),
            icon: icon.map(|s| s.to_string()),
        }
    }

    pub fn to_web_push_message(
        &self,
        sub_info: &SubscriptionInfo,
    ) -> Result<WebPushMessage, WebPushError> {
        let sig_builder = VapidSignatureBuilder::from_base64(
            "KaRfTAcDs9ztATKecCL_mBJYdO57X3NvzgWnBNTBQ4c",
            URL_SAFE,
            sub_info,
        )?
        .build()?;

        let content_as_bytes: Vec<u8> = self.into();

        let mut builder = WebPushMessageBuilder::new(sub_info);
        builder.set_payload(ContentEncoding::Aes128Gcm, &content_as_bytes);
        builder.set_vapid_signature(sig_builder);

        builder.build()
    }
}
