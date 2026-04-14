fn print_usage() {
    eprintln!(
        "Usage: chatbot-ctl <command>\n\n\
         Instance commands:\n\
         \x20 launch [--provider <p>] [--instance N] [--cwd DIR] [--window-x X --window-y Y --window-width W --window-height H]\n\
         \x20                    Launch a UI instance (provider: minimax|codex|geminiforge|geminicliforge|minimaxforge|kimi; defaults to [provider].default_provider)\n\
         \x20 send [--provider <p>] --chat-id <id> --text <prompt>\n\
         \x20                    Append a user message to a session and start a worker (defaults to [provider].default_provider)\n\
         \x20 restart-running    Restart currently running UI instances\n\
         \x20 status              Show broker + instances + worker jobs\n\
         \x20 stop [PID...]       Stop UI instances (all if no PIDs given)\n\
         \x20 pids                Print live PIDs only (for scripting)\n\
         \x20 stop-all            Stop workers + UI instances + broker\n\n\
         Backup commands:\n\
         \x20 ensure-backup-scheduler\n\
         \x20                    Internal helper; refreshes the managed backup scheduler for this platform\n\
         \x20 run-managed-backup [--force]\n\
         \x20                    Internal scheduler entrypoint; runs the managed backup when due\n\n\
         Worker commands:\n\
         \x20 workers             List worker jobs (from spool)\n\
         \x20 stop-workers        SIGTERM all running workers\n\
         \x20 stop-workers --chat-id <id>   Stop specific worker\n\
         \x20 gc                  Clean up stale worker jobs\n\n\
         Desktop commands:\n\
         \x20 install-desktop     Write .desktop entries to ~/.local/share/applications\n\
         \x20 uninstall-desktop   Remove .desktop entries\n\n\
         Broker commands:\n\
         \x20 install-broker      Write systemd user unit + enable + start\n\
         \x20 uninstall-broker    Disable + remove systemd unit\n\
         \x20 start-broker        Start broker via systemd\n\
         \x20 stop-broker         Stop broker via systemd\n\
         \x20 restart-broker      Restart broker via systemd\n\
         \x20 broker-status       Show broker systemd status\n\
         \x20 broker-logs         Follow broker journal logs\n\n\
         Database commands:\n\
         \x20 rebuild-index      Repopulate SQLite from authoritative files\n\
         \x20 runs list         List indexed autonomous runs when present\n\
         \x20 stats             Show provider success data and cost-tier breakdown\n\
         \x20 prune --age DAYS  Moved to the auto-runs app\n"
    );
}
