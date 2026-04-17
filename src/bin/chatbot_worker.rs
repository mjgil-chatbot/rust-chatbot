fn main() {
    chatbot_lib::process_limits::apply_default_process_memory_limit("chatbot-worker");
    chatbot_lib::chatbot_worker::main();
}
