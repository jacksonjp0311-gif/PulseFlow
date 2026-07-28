fn main() {
    println!("cargo:rerun-if-changed=assets/icons/pulseflow-governor.ico");

    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("assets/icons/pulseflow-governor.ico");
        resource.set("ProductName", "PulseFlow Governor");
        resource.set("FileDescription", "PulseFlow Governor");
        resource.set("LegalCopyright", "Copyright James Paul Jackson");
        resource
            .compile()
            .expect("failed to embed the PulseFlow Governor Windows icon");
    }
}
