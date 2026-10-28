//! The tune language, served to the `fig` CLI as a helper process over
//! stdio. Configured in .fig/languages.figl.

fn main() -> std::io::Result<()> {
    fig::helper::serve(tune::figlang::Tune)
}
