CREATE TABLE licenses (
  lid TEXT PRIMARY KEY,
  key TEXT NOT NULL,
  email TEXT NOT NULL,
  name TEXT NOT NULL,
  session_id TEXT NOT NULL UNIQUE,
  payment_intent TEXT,
  revoked INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE INDEX licenses_email ON licenses (email);
CREATE INDEX licenses_pi ON licenses (payment_intent);
