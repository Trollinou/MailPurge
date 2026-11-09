#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImapAuthMethod {
    Password(String),
    XOAuth2 { access_token: String },
}

#[derive(Debug, Clone)]
pub struct ImapConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub auth: ImapAuthMethod,
}

pub struct XOAuth2Authenticator {
    pub username: String,
    pub access_token: String,
}

impl imap::Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&self, _data: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.username, self.access_token
        )
    }
}
