fn default_instance_for_provider(provider: ModelProvider) -> u32 {
    match provider {
        ModelProvider::Claude => 1,
        ModelProvider::MiniMax => 2,
        ModelProvider::Qwen => 5,
        ModelProvider::Codex => 3,
        ModelProvider::Nvidia => 7,
        ModelProvider::Gemini => 4,
        ModelProvider::OpenRouter => 8,
        ModelProvider::OmniForge => 11,
        ModelProvider::GeminiForge => 9,
        ModelProvider::GeminiCliForge => 12,
        ModelProvider::MiniMaxForge => 10,
        ModelProvider::Kimi => 13,
        ModelProvider::OpenCode => 14,
        ModelProvider::Hermes => 15,
        ModelProvider::Forge => 16,
        ModelProvider::Claw => 17,
    }
}
