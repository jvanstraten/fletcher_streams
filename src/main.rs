extern crate pest;
#[macro_use]
extern crate pest_derive;

use pest::Parser;

#[derive(Parser)]
#[grammar = "type.pest"]
pub struct TypeParser;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("{}", TypeParser::parse(Rule::Type, &args[1]).unwrap());
}
