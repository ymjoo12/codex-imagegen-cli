pub fn redact_known_secrets(mut text: String, secrets: &[String]) -> String {
    for secret in secrets {
        if secret.len() >= 8 {
            text = text.replace(secret, "[redacted]");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_long_secret_values() {
        let redacted = redact_known_secrets(
            "token abcdefghijkl appears".to_string(),
            &["abcdefghijkl".to_string()],
        );
        assert_eq!(redacted, "token [redacted] appears");
    }

    #[test]
    fn ignores_short_values() {
        let redacted = redact_known_secrets("id abc".to_string(), &["abc".to_string()]);
        assert_eq!(redacted, "id abc");
    }
}
