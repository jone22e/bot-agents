/// Gera um identificador UUID v7 (ordenavel no tempo) em texto.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub fn is_valid_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_unique_ids() {
        let a = new_id();
        let b = new_id();
        assert!(is_valid_id(&a));
        assert!(is_valid_id(&b));
        assert_ne!(a, b);
    }
}
