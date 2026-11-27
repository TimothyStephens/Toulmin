CREATE TABLE head (chain TEXT PRIMARY KEY, last_id TEXT NOT NULL, seq INTEGER NOT NULL,
  segment INTEGER, offset INTEGER, batch INTEGER, epoch INTEGER, schema_version INTEGER NOT NULL);
CREATE TABLE events (id TEXT PRIMARY KEY, chain TEXT NOT NULL, seq INTEGER NOT NULL, type TEXT NOT NULL,
  segment INTEGER NOT NULL, offset INTEGER NOT NULL, at TEXT NOT NULL);
CREATE INDEX events_chain_seq ON events(chain, seq);
CREATE TABLE opaque_events (id TEXT PRIMARY KEY, type TEXT NOT NULL, tv INTEGER NOT NULL);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT, event_id TEXT NOT NULL);
CREATE TABLE files (file TEXT PRIMARY KEY, kind TEXT NOT NULL, path TEXT NOT NULL, title TEXT, description TEXT,
  source TEXT, level TEXT NOT NULL, retired INTEGER NOT NULL DEFAULT 0, restored INTEGER NOT NULL DEFAULT 0,
  removable INTEGER NOT NULL DEFAULT 0, agent_denied INTEGER NOT NULL DEFAULT 0, registered_event TEXT);
CREATE INDEX files_path ON files(path);
CREATE TABLE file_versions (version TEXT PRIMARY KEY, file TEXT NOT NULL, n INTEGER NOT NULL, fingerprint TEXT NOT NULL,
  origin_kind TEXT NOT NULL, origin_run TEXT, content_label TEXT, size INTEGER, kept_object TEXT,
  fingerprint_only INTEGER NOT NULL DEFAULT 0, decompressed_fingerprint TEXT, event_id TEXT NOT NULL);
CREATE INDEX file_versions_file ON file_versions(file, n);
CREATE TABLE parse_specs (file TEXT PRIMARY KEY, spec TEXT NOT NULL, spec_fingerprint TEXT NOT NULL, event_id TEXT NOT NULL);
CREATE TABLE panels (version TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY (version, name));
CREATE TABLE reports (report TEXT PRIMARY KEY, path TEXT NOT NULL, event_id TEXT NOT NULL);
CREATE TABLE spans (report TEXT NOT NULL, ref TEXT NOT NULL, line INTEGER NOT NULL, pos INTEGER NOT NULL,
  sentence TEXT NOT NULL, inline_code INTEGER NOT NULL DEFAULT 0, section_kind TEXT, attributes TEXT NOT NULL,
  PRIMARY KEY (report, ref, line, pos));
CREATE TABLE claims (ref TEXT PRIMARY KEY, id TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, -- added, reviewed, retracted, superseded (gate A)
  created_event TEXT NOT NULL, current_revision TEXT NOT NULL, report TEXT, supersedes TEXT, superseded_by TEXT);
CREATE TABLE claim_revisions (event_id TEXT PRIMARY KEY, ref TEXT NOT NULL, sentence TEXT NOT NULL, cited TEXT NOT NULL,
  citations TEXT NOT NULL, steps TEXT, assertion TEXT, qualifiers TEXT, at TEXT NOT NULL);
CREATE INDEX claim_revisions_ref ON claim_revisions(ref, at);
CREATE TABLE claim_cites (ref TEXT NOT NULL, version TEXT NOT NULL, PRIMARY KEY (ref, version));
CREATE TABLE claim_keys (ref TEXT NOT NULL, version TEXT NOT NULL, key_tuple TEXT NOT NULL, PRIMARY KEY (ref, version, key_tuple));
CREATE TABLE claim_events (ref TEXT NOT NULL, event_id TEXT NOT NULL, type TEXT NOT NULL, at TEXT NOT NULL, PRIMARY KEY (ref, event_id));
CREATE TABLE audits (event_id TEXT PRIMARY KEY, at TEXT NOT NULL, since TEXT, report_hash TEXT NOT NULL, counts TEXT NOT NULL);
CREATE TABLE verdicts (event_id TEXT PRIMARY KEY, ref TEXT NOT NULL, revision TEXT NOT NULL, against TEXT NOT NULL,
  evidence_outcome TEXT NOT NULL, assertion_outcome TEXT, body TEXT NOT NULL, at TEXT NOT NULL);
CREATE INDEX verdicts_ref ON verdicts(ref, at);
CREATE TABLE reviews (event_id TEXT PRIMARY KEY, ref TEXT NOT NULL, revision TEXT NOT NULL, outcome TEXT NOT NULL, note TEXT, at TEXT NOT NULL);
CREATE TABLE flags (subject_kind TEXT NOT NULL, subject TEXT NOT NULL, name TEXT NOT NULL, reason TEXT NOT NULL,
  since_event TEXT NOT NULL, PRIMARY KEY (subject_kind, subject, name));
