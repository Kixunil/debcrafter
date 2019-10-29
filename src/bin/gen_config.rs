mod codegen;
mod generator;

fn main() {
    codegen::generate(codegen::GenFileName::Extension("config"), generator::config::generate);
}
