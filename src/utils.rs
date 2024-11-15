pub fn mod_exp(base: u16, exponent: u16, modulus: u16) -> u16 {
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
