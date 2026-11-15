-- One active device per license. PRIMARY KEY(lid) ⇒ a license can hold at most one activation.
-- Transfer = delete the row (POST /deactivate) so another device can insert it.
CREATE TABLE activations (
  lid TEXT PRIMARY KEY,
  device_hash TEXT NOT NULL,
  device_name TEXT,
  created_at INTEGER NOT NULL
);
