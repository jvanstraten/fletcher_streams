extern crate pest;
#[macro_use]
extern crate pest_derive;

use pest::Parser;

#[derive(Parser)]
#[grammar = "type.pest"]
pub struct TypeParser;

const fn num_bits<T>() -> usize {
    std::mem::size_of::<T>() * 8
}

fn log2(x: usize) -> u32 {
    assert!(x > 0);
    num_bits::<usize>() as u32 - (x - 1).leading_zeros()
}

#[derive(Debug)]
struct UnionOption {
    name: String,
    element_type: Vec<Field>,
}

impl UnionOption {
    fn null() -> UnionOption {
        UnionOption {
            name: "<NULL>".to_string(),
            element_type: vec![],
        }
    }
}

enum FieldType {
    Bits(u32),
    Length,
    Union(Vec<UnionOption>),
}

impl FieldType {
    fn option_width(&self) -> u32 {
        match self {
            FieldType::Bits(_) => 0,
            FieldType::Length => 0,
            FieldType::Union(options) => log2(options.len()),
        }
    }

    fn data_width(&self) -> u32 {
        match self {
            FieldType::Bits(w) => *w,
            FieldType::Length => 32,
            FieldType::Union(options) => options
                .iter()
                .map(|option| option.element_type.iter().fold(0, |acc, field| acc + field.typ.width()))
                .fold(0, |max, width| std::cmp::max(max, width)),
        }
    }

    fn width(&self) -> u32 {
        self.option_width() + self.data_width()
    }
}

impl std::fmt::Debug for FieldType {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FieldType::Bits(w) => write!(fmt, "b{}", w),
            FieldType::Length => write!(fmt, "vector_length"),
            FieldType::Union(options) => {
                fmt.debug_list().entries(options.iter()).finish()
            }
        }
    }
}

struct Field {
    name: String,
    typ: FieldType,
    start: usize,
    end: usize,
}

impl std::fmt::Debug for Field {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("Field")
            .field("name", &self.name)
            .field("option_width", &self.typ.option_width())
            .field("data_width", &self.typ.data_width())
            .field("type", &self.typ)
            .finish()
    }
}

impl Field {
    pub fn new(typ: FieldType, name: Option<String>, span: pest::Span) -> Field {
        Field {
            name: name.unwrap_or("root".to_string()),
            typ,
            start: span.start(),
            end: span.end(),
        }
    }
}

struct Stream {
    name: String,
    element_type: Vec<Field>,
    dimensionality: u32,
}

impl Stream {
    pub fn new(name: Option<String>, dimensionality: u32) -> Stream {
        Stream {
            name: name.unwrap_or("root".to_string()),
            element_type: vec![],
            dimensionality,
        }
    }
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("Stream")
            .field("name", &self.name)
            .field("dimensionality", &self.dimensionality)
            .field("element_width", &self.element_type.iter().fold(0, |acc, field| acc + field.typ.width()))
            .field("element_type", &self.element_type)
            .finish()
    }
}

struct StreamBundle {
    primary: Stream,
    secondary: Vec<Stream>,
}

impl StreamBundle {
    pub fn new(name: Option<String>) -> StreamBundle {
        StreamBundle {
            primary: Stream::new(name, 0),
            secondary: vec![],
        }
    }
}

impl std::fmt::Debug for StreamBundle {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_list().entry(&self.primary).entries(self.secondary.iter()).finish()
    }
}


fn to_bundle(node: pest::iterators::Pair<Rule>, name: Option<&str>) -> StreamBundle {
    assert_eq!(node.as_rule(), Rule::Element);

    let mut name = name.map(String::from);

    for node in node.into_inner() {
        match node.as_rule() {
            Rule::Name => {
                name = Some(if let Some(name) = name {
                    name + "." + node.as_str()
                } else {
                    node.as_str().to_string()
                });
            }
            Rule::Bits => {
                let mut bundle = StreamBundle::new(name.clone());
                let typ = FieldType::Bits(node.as_str()[1..].parse().unwrap());
                let field = Field::new(typ, name, node.as_span());
                bundle.primary.element_type.push(field);
                return bundle;
            }
            Rule::Struct => {
                let mut bundle = StreamBundle::new(name.clone());
                for elem in node.into_inner() {
                    let mut sub = to_bundle(elem, name.as_ref().map(|x| &**x));
                    if sub.primary.dimensionality == 0 {
                        bundle.primary.element_type.extend(sub.primary.element_type.drain(..));
                    } else {
                        bundle.secondary.push(sub.primary);
                    }
                    bundle.secondary.extend(sub.secondary.drain(..));
                }
                if bundle.primary.element_type.is_empty() {
                    bundle.primary = bundle.secondary.remove(0);
                }
                return bundle;
            }
            Rule::Union => {
                let mut bundle = StreamBundle::new(name.clone());
                let mut options = vec![];
                let span = node.as_span();
                for elem in node.into_inner() {
                    if elem.as_rule() == Rule::Null {
                        options.push(UnionOption::null());
                    } else {
                        let mut sub = to_bundle(elem, name.as_ref().map(|x| &**x));
                        if sub.primary.dimensionality == 0 {
                            options.push(UnionOption {
                                name: sub.primary.name,
                                element_type: sub.primary.element_type,
                            });
                        } else {
                            options.push(UnionOption {
                                name: sub.primary.name.clone(),
                                element_type: vec![],
                            });
                            bundle.secondary.push(sub.primary);
                        }
                        bundle.secondary.extend(sub.secondary.drain(..));
                    }
                }
                let typ = FieldType::Union(options);
                let field = Field::new(typ, name, span);
                bundle.primary.element_type.push(field);
                return bundle;
            }
            Rule::List => {
                let elem = node.into_inner().next().unwrap();
                let mut bundle = to_bundle(elem, name.map(|x| x + "[]").as_ref().map(|x| &**x));
                bundle.primary.dimensionality += 1;
                for ref mut stream in &mut bundle.secondary {
                    stream.dimensionality += 1;
                }
                return bundle;
            }
            Rule::Vector => {
                let mut bundle = StreamBundle::new(name.clone());
                let typ = FieldType::Length;
                let field = Field::new(typ, name.clone(), node.as_span());
                bundle.primary.element_type.push(field);
                let elem = node.into_inner().next().unwrap();
                let sub = to_bundle(elem, name.map(|x| x + "<>").as_ref().map(|x| &**x));
                bundle.secondary.push(sub.primary);
                bundle.secondary.extend(sub.secondary);
                return bundle;
            }
            _ => unreachable!(),
        }
    }
    unreachable!();
}

fn main() {
    // for example:
    //   x: {NULL, banana: <(a: b10, b: b29)>, triangle: [[(y: <b1>, z: b2)]]}
    let args: Vec<String> = std::env::args().collect();
    let mut parse_result = TypeParser::parse(Rule::Root, &args[1]).unwrap();
    dbg!(to_bundle(parse_result.next().unwrap(), None));
}
