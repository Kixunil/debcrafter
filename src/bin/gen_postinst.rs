mod codegen;
mod generator;

fn main() {
    codegen::generate(codegen::GenFileName::Extension("postinst"), generator::postinst::generate);
}
