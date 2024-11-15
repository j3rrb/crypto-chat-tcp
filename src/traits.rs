pub trait Encryptor: Send + Sync {
    fn encrypt(&self, message: &str, shared_key: &u16) -> Result<String, String>;
}

pub trait Decryptor: Send + Sync {
    fn decrypt(&self, message: &str, shared_key: &u16) -> Result<String, String>;
}
