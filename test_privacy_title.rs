use opencode_mem_core::filter_private_content;

fn main() {
    let raw = "Title with <private>secret</private> and stuff";
    let filtered = filter_private_content(raw);
    println!("Filtered: {}", filtered);
}
