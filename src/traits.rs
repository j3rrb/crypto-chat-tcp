pub trait Encryptor: Send + Sync {
    fn encrypt(&self, message: &str) -> Result<String, String>;
}

pub trait Decryptor: Send + Sync {
    fn decrypt(&self, message: &str) -> Result<String, String>;
}
