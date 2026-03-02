fn main() {
    let query = "src/utils.rs hello-world user.name@email.com ___ foo bar_baz";
    let words: Vec<_> = query.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty() && w.chars().any(char::is_alphanumeric))
        .collect();
    println!("{:?}", words);
}
