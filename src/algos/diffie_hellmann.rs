use rand::{self, Rng};

use crate::algos::ceasar::caesar_cipher;
use crate::constants::{BASE, PRIME};
use crate::structs::DiffieHellman;
use crate::traits::{Decryptor, Encryptor};

fn mod_exp(base: u32, exponent: u32, modulus: u32) -> u32 {
    let mut result = 1;
    let mut base = base % modulus;
    let mut exp = exponent;

    while exp > 0 {
        if exp % 2 == 1 {
            result = (result * base) % modulus;
        }

        base = (base * base) % modulus;
        exp /= 2;
    }

    result
}

impl DiffieHellman {
    pub fn new() -> Self {
        let private_key: u32 = rand::thread_rng().gen_range(1..=20);
        let public_key: u32 = mod_exp(BASE, private_key, PRIME);

        DiffieHellman {
            private_key,
            public_key,
        }
    }
}

impl Encryptor for DiffieHellman {
    fn encrypt(&self, message: &str) -> Result<String, String> {
        let shared_key = mod_exp(self.public_key, self.private_key, PRIME);
        let encrypted_msg = caesar_cipher(message, shared_key as usize);

        Ok(encrypted_msg)
    }
}

impl Decryptor for DiffieHellman {
    fn decrypt(&self, message: &str) -> Result<String, String> {
        let shared_key = mod_exp(self.public_key, self.private_key, PRIME);
        let decrypted_msg = caesar_cipher(message, 26 - (shared_key as usize));

        Ok(decrypted_msg)
    }
}
