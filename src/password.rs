use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHasher as _};

pub fn generate_hash(plaintext_password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(plaintext_password.as_bytes(), &salt)
        .unwrap();
    password_hash.to_string()
}

pub fn validate_password(password: &str) -> String {
    if password.len() < 8 || password.len() > 60 || !password.is_ascii() {
        String::from(
            "Password must be between 8 and 60 characters and only contain ascii characters.",
        )
    } else {
        String::new()
    }
}

pub fn validate_username(username: &str) -> String {
    if username.len() < 5
        || username.len() > 20
        || !username.chars().all(char::is_alphanumeric)
    {
        String::from(
            "Username must be between 5 and 20 characters, and only contain letters / numbers.",
        )
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_too_short() {
        assert!(!validate_password("short").is_empty());
        assert!(!validate_password("1234567").is_empty());
    }

    #[test]
    fn password_too_long() {
        let long_password = "a".repeat(61);
        assert!(!validate_password(&long_password).is_empty());
    }

    #[test]
    fn password_non_ascii() {
        assert!(!validate_password("password🔒").is_empty());
        assert!(!validate_password("пароль1234").is_empty());
    }

    #[test]
    fn password_valid() {
        assert!(validate_password("password123").is_empty());
        assert!(validate_password("12345678").is_empty());
        assert!(validate_password(&"a".repeat(60)).is_empty());
    }

    #[test]
    fn username_too_short() {
        assert!(!validate_username("abcd").is_empty());
        assert!(!validate_username("a").is_empty());
    }

    #[test]
    fn username_too_long() {
        assert!(!validate_username(&"a".repeat(21)).is_empty());
    }

    #[test]
    fn username_invalid_chars() {
        assert!(!validate_username("user_name").is_empty());
        assert!(!validate_username("user-name").is_empty());
        assert!(!validate_username("user name").is_empty());
        assert!(!validate_username("user@name").is_empty());
    }

    #[test]
    fn username_valid() {
        assert!(validate_username("validuser").is_empty());
        assert!(validate_username("user123").is_empty());
        assert!(validate_username("12345").is_empty());
        assert!(validate_username(&"a".repeat(20)).is_empty());
    }

    #[test]
    fn hash_and_verify() {
        use argon2::{Argon2, PasswordHash, PasswordVerifier as _};

        let password = "testpassword123";
        let hash = generate_hash(password);

        let parsed = PasswordHash::new(&hash).expect("valid hash");
        assert!(
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        );
    }

    #[test]
    fn hash_wrong_password_fails() {
        use argon2::{Argon2, PasswordHash, PasswordVerifier as _};

        let hash = generate_hash("correctpassword");
        let parsed = PasswordHash::new(&hash).expect("valid hash");

        assert!(
            Argon2::default()
                .verify_password(b"wrongpassword", &parsed)
                .is_err()
        );
    }
}
