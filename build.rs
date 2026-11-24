fn main() {
    let config = slint_build::CompilerConfiguration::default().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app-window.slint", config)
        .expect("failed to compile Slint UI");

    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/tray/zapret-on.ico");
        resource.set_manifest_file("assets/app.manifest");
        resource
            .compile_for("zapret-gui")
            .expect("failed to compile Windows resources");
    }
}
