fn chatbot_binary_path() -> PathBuf {
    resolve_binary_in_current_profile(&[chatbot_lib::config::UI_BINARY_NAME])
        .unwrap_or_else(exit_on_binary_resolution_error)
}
