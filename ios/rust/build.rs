#[path = "../../src/native/build.rs"]
mod apple;
fn main() {
    apple::build(std::path::Path::new("../.."));
}
