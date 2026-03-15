use regex::Regex;

fn main() {
    let re = Regex::new(r"(?s)\*\*Observation:\*\*\s*(.*?)(?:\*\*|$)").unwrap();
    let md = "**Observation:** This is an observation with **bold** text in it.";
    if let Some(caps) = re.captures(md) {
        println!("Extracted: {:?}", caps.get(1).map(|m| m.as_str()));
    }
}
