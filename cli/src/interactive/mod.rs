//! Interactive prompt primitives shared by the wiring flows.

pub(crate) mod scope;
pub(crate) mod wire;

/// Prompt on stdout and read one trimmed line from stdin. Returns None on EOF.
pub(crate) fn ask(stdin: &std::io::Stdin, msg: &str) -> std::io::Result<Option<String>> {
    use std::io::{BufRead, Write};
    print!("{msg}");
    std::io::stdout().flush().ok();
    let mut s = String::new();
    if stdin.lock().read_line(&mut s)? == 0 {
        return Ok(None);
    }
    Ok(Some(s.trim().to_string()))
}

/// Ask a yes/no question (default no). Returns true only on an explicit yes.
pub(crate) fn ask_yes(stdin: &std::io::Stdin, msg: &str) -> std::io::Result<bool> {
    Ok(ask(stdin, msg)?
        .map(|a| matches!(a.to_lowercase().as_str(), "y" | "yes"))
        .unwrap_or(false))
}
