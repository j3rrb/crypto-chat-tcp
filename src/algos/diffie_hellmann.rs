use crate::algos::ceasar::caesar_cipher;
use crate::structs::DiffieHellman;
use crate::traits::{Decryptor, Encryptor};

impl DiffieHellman {
    pub fn new() -> Self {
        DiffieHellman {}
    }
}

impl Encryptor for DiffieHellman {
    fn encrypt(&self, message: &str, shared_key: &u32) -> Result<String, String> {
        let encrypted_msg = caesar_cipher(message, *shared_key);

        Ok(encrypted_msg)
    }
}

impl Decryptor for DiffieHellman {
    fn decrypt(&self, message: &str, shared_key: &u32) -> Result<String, String> {
        let decrypted_msg = caesar_cipher(message, 26 - (shared_key % 26));

        Ok(decrypted_msg)
    }
}
