use extendr_api::prelude::*;

pub(super) fn strings_arg(
    value: &Robj,
    name: &str,
) -> std::result::Result<Option<Vec<String>>, String> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str_vector()
        .map(|values| Some(values.into_iter().map(str::to_owned).collect()))
        .ok_or_else(|| format!("Failed to parse '{name}': must be a character"))
}

pub(super) fn u8_to_list_rstr(values: Vec<Vec<u8>>) -> Vec<Rstr> {
    values.into_iter().map(u8_to_rstr).collect()
}

pub(super) fn u8_to_rstr(bytes: Vec<u8>) -> Rstr {
    Rstr::from_string(&String::from_utf8_lossy(&bytes))
}
