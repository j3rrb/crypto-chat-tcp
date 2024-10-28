pub fn caesar_cipher(text: &str, shift: usize) -> String {
    let shift = shift % 26;

    text.chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_lowercase() { b'a' } else { b'A' };
                let new_c = ((c as u8 - base + shift as u8) % 26) + base;
                new_c as char
            } else {
                c
            }
        })
        .collect()
}
