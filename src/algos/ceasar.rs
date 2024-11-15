pub fn caesar_cipher(text: &str, shift: u16) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let a = if c.is_ascii_lowercase() { b'a' } else { b'A' };
                let shifted = (c as u8 - a + (shift % 26) as u8) % 26 + a;
                shifted as char
            } else {
                c
            }
        })
        .collect()
}
