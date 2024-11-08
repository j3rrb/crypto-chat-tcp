pub fn mod_exp(base: u32, exponent: u32, modulus: u32) -> u32 {
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
