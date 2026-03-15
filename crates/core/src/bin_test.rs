fn main() {
    let title = opencode_mem_core::strip_uuid_from_title("b3b61de2-1234-5678-9abc-def012345678");
    println!("UUID-only title becomes: '{}'", title);
}
