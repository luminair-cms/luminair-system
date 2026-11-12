-- no-transaction
-- Migration: 20261010000002_create_document_snapshots.sql
-- Description: Append-only historical revision archive (ADR-004)

CREATE TABLE IF NOT EXISTS document_snapshots (
    id               UUID PRIMARY KEY,
    document_type_id VARCHAR(64) NOT NULL,
    document_id      UUID NOT NULL,
    revision         INT NOT NULL,
    content          JSONB NOT NULL,
    relations        JSONB NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL,
    created_by       VARCHAR(255) NOT NULL,
    reason           VARCHAR(32) NOT NULL CHECK (reason IN ('publish', 'unpublish')),
    CONSTRAINT uq_document_snapshots_revision UNIQUE (document_type_id, document_id, revision)
);

CREATE INDEX IF NOT EXISTS idx_document_snapshots_lookup
    ON document_snapshots (document_type_id, document_id, revision DESC);
