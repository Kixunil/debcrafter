mod codegen;
mod generator;

fn main() {
    codegen::generate(codegen::GenFileName::Raw("control"), generator::control::generate);
}
