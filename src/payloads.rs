use rand::{Rng, distributions::Alphanumeric};

pub fn generate_payload_id() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub fn fqdn(payload_id: &str, root_domain: &str) -> String {
    format!("{}.{}", payload_id, root_domain.trim_end_matches('.'))
}

pub fn extract_payload_id(host_or_name: &str, root_domain: &str) -> Option<String> {
    let normalized = host_or_name.trim_end_matches('.').to_ascii_lowercase();
    let root = root_domain.trim_end_matches('.').to_ascii_lowercase();
    let suffix = format!(".{root}");

    normalized
        .strip_suffix(&suffix)
        .and_then(|prefix| prefix.split('.').next())
        .filter(|payload_id| !payload_id.is_empty())
        .map(ToOwned::to_owned)
}
