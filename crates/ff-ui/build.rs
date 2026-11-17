fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("fastfile.ico");
        res.set("ProductName", "FastFile");
        res.set(
            "FileDescription",
            "FastFile — Busca, Prévia, Diff e Uso de Disco",
        );
        res.set("LegalCopyright", "Copyright (C) 2026 Fabio Motta");
        if let Err(e) = res.compile() {
            eprintln!("Aviso: falha ao compilar recursos winres: {e}");
        }
    }
}
