mod codegen;
mod generator;

fn main() {
    codegen::generate(codegen::GenFileName::Extension("templates"), generator::templates::generate);
}
