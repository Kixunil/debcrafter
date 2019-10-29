mod codegen;
mod generator;

fn main() {
    codegen::generate(codegen::GenFileName::Extension("service"), generator::service::generate);
}
