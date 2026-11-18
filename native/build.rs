// The icon as a Windows resource, so the exe, its shortcuts and the taskbar show it without assets/ next to it.
fn main() {
    println!("cargo:rerun-if-changed=../assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/icon.ico");
        res.compile().expect("ícone do executável");
    }
}
