mod codegen;
mod generator;

fn main() {
    codegen::generate(codegen::GenFileName::Extension("triggers"), generator::triggers::generate);
}
