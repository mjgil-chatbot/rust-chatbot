// ── database management ─────────────────────────────────────────────

fn cmd_rebuild_index() {
    println!("Rebuilding SQLite index from authoritative files...");

    match chatbot_lib::storage::rebuild_all() {
        Ok(report) => {
            println!("Index rebuild complete.");
            println!("  Sessions:       {}", report.sessions_indexed);

            if !report.drifted_tables.is_empty() {
                println!(
                    "\nIntegrity check: {} table(s) drifted from authoritative files.",
                    report.drifted_tables.len()
                );
                for table in &report.drifted_tables {
                    println!("  - {table}");
                }
            }

            if !report.warnings.is_empty() {
                println!("\nWarnings:");
                for warning in report.warnings {
                    println!("  - {}", warning);
                }
            }
        }
        Err(err) => {
            eprintln!("Failed to rebuild index: {err}");
            process::exit(1);
        }
    }
}

fn cmd_runs_list(
    status_filter: Option<&str>,
    provider_filter: Option<&str>,
    limit_filter: Option<usize>,
    workflow_filter: Option<&str>,
) {
    let Some(conn) = open_legacy_runs_index(false) else {
        println!("No indexed autonomous runs found.");
        return;
    };

    let runs = match list_legacy_runs(
        &conn,
        status_filter,
        provider_filter,
        workflow_filter,
        limit_filter,
    ) {
        Ok(runs) => runs,
        Err(err) => {
            eprintln!("Failed to query runs: {err}");
            return;
        }
    };

    if runs.is_empty() {
        println!("No indexed autonomous runs found.");
        return;
    }

    println!(
        "{:<36} {:<10} {:<10} {:<10} {:<12} {}",
        "RUN ID", "PROVIDER", "STATUS", "WORKFLOW", "CREATED", "GOAL"
    );
    println!("{}", "-".repeat(110));
    for run in runs {
        println!(
            "{:<36} {:<10} {:<10} {:<10} {:<12} {}",
            run.run_id,
            run.provider.as_deref().unwrap_or(""),
            run.status,
            run.workflow_mode.as_deref().unwrap_or(""),
            run.created_at.split('T').next().unwrap_or(""),
            summarize_goal(run.goal.as_deref())
        );
    }
}

fn cmd_stats() {
    println!("Auto-run usage analytics moved to the auto-runs app.");
}

fn cmd_prune(age_days: u32) {
    let _ = age_days;
    println!("Autonomous run pruning moved to the auto-runs app.");
}

#[derive(Debug, Default)]
struct LegacyRunRecord {
    run_id: String,
    provider: Option<String>,
    status: String,
    workflow_mode: Option<String>,
    created_at: String,
    goal: Option<String>,
}

fn open_legacy_runs_index(write: bool) -> Option<rusqlite::Connection> {
    let path = chatbot_lib::storage::index_db_path();
    if !path.is_file() {
        return None;
    }

    let flags = if write {
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let conn = match rusqlite::Connection::open_with_flags(&path, flags) {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("Failed to open {}: {err}", path.display());
            return None;
        }
    };

    match legacy_table_exists(&conn, "runs") {
        Ok(true) => Some(conn),
        Ok(false) => None,
        Err(err) => {
            eprintln!("Failed to inspect {}: {err}", path.display());
            None
        }
    }
}

fn legacy_table_exists(conn: &rusqlite::Connection, table: &str) -> rusqlite::Result<bool> {
    let exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(exists != 0)
}

fn list_legacy_runs(
    conn: &rusqlite::Connection,
    status_filter: Option<&str>,
    provider_filter: Option<&str>,
    workflow_filter: Option<&str>,
    limit_filter: Option<usize>,
) -> rusqlite::Result<Vec<LegacyRunRecord>> {
    let mut sql =
        String::from("SELECT run_id, provider, status, workflow_mode, created_at, goal FROM runs");
    let mut filters = Vec::new();
    let mut values = Vec::new();

    if let Some(status) = status_filter {
        filters.push("status = ?".to_string());
        values.push(rusqlite::types::Value::Text(status.to_string()));
    }
    if let Some(provider) = provider_filter {
        filters.push("provider = ?".to_string());
        values.push(rusqlite::types::Value::Text(provider.to_string()));
    }
    if let Some(workflow) = workflow_filter {
        filters.push("workflow_mode = ?".to_string());
        values.push(rusqlite::types::Value::Text(workflow.to_string()));
    }
    if !filters.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&filters.join(" AND "));
    }
    sql.push_str(" ORDER BY created_at DESC, run_id DESC");
    if let Some(limit) = limit_filter {
        sql.push_str(" LIMIT ?");
        values.push(rusqlite::types::Value::Integer(limit as i64));
    }

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
        Ok(LegacyRunRecord {
            run_id: row.get(0)?,
            provider: row.get(1)?,
            status: row.get(2)?,
            workflow_mode: row.get(3)?,
            created_at: row.get(4)?,
            goal: row.get(5)?,
        })
    })?;

    let mut runs = Vec::new();
    for row in rows {
        runs.push(row?);
    }
    Ok(runs)
}

fn summarize_goal(goal: Option<&str>) -> String {
    goal.unwrap_or("").chars().take(40).collect()
}
