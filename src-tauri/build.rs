fn main() {
    // Les commandes applicatives sont déclarées ici : Tauri génère pour chacune une permission
    // `allow-<commande>`, et une commande n'est appelable que si une capability l'accorde
    // explicitement (src-tauri/capabilities/). Toute nouvelle commande doit aussi être ajoutée
    // à la liste blanche de l'isolation (isolation/index.js) ; un test vérifie la cohérence.
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "app_info",
            "ssh_connect",
            "ssh_write",
            "ssh_resize",
            "ssh_ack",
            "ssh_host_key_decision",
            "ssh_close",
            "ssh_pick_key_file",
            "open_external_url",
        ]));

    if let Err(error) = tauri_build::try_build(attributes) {
        eprintln!("échec de tauri-build : {error:#}");
        std::process::exit(1);
    }
}
