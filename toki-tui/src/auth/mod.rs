// Auth module placeholder
// Future: Add session cookie reuse, device code flow, or credential storage

pub struct Credentials {
    pub user_id: i32,
}

impl Credentials {
    pub fn demo() -> Self {
        Self { user_id: 1 }
    }
}
