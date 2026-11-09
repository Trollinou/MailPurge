/// Formats a list of IMAP UIDs into a compressed sequence set (e.g. "1:3,7,9:10").
/// Duplicates are removed and values are sorted automatically.
pub fn format_sequence_set(uids: &[u32]) -> String {
    if uids.is_empty() {
        return String::new();
    }

    let mut sorted_uids = uids.to_vec();
    sorted_uids.sort_unstable();
    sorted_uids.dedup();

    let mut parts: Vec<String> = Vec::new();
    let mut range_start = sorted_uids[0];
    let mut range_end = sorted_uids[0];

    for &uid in &sorted_uids[1..] {
        if uid == range_end + 1 {
            range_end = uid;
        } else {
            push_range(&mut parts, range_start, range_end);
            range_start = uid;
            range_end = uid;
        }
    }
    push_range(&mut parts, range_start, range_end);

    parts.join(",")
}

fn push_range(parts: &mut Vec<String>, start: u32, end: u32) {
    if start == end {
        parts.push(start.to_string());
    } else {
        parts.push(format!("{}:{}", start, end));
    }
}

/// Splits a list of UIDs into batches of at most `max_chunk_size` elements
/// and formats each batch into a compressed IMAP sequence set.
pub fn chunk_sequence_set(uids: &[u32], max_chunk_size: usize) -> Vec<String> {
    if uids.is_empty() || max_chunk_size == 0 {
        return Vec::new();
    }

    let mut sorted_uids = uids.to_vec();
    sorted_uids.sort_unstable();
    sorted_uids.dedup();

    sorted_uids
        .chunks(max_chunk_size)
        .map(format_sequence_set)
        .collect()
}
