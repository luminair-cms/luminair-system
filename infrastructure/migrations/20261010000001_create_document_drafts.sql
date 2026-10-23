-- no-transaction
-- Migration: 20261010000001_create_document_drafts.sql
-- Description: Unified draft store for uncommitted document working copies (ADR-004)

CREATE TABLE IF NOT EXISTS document_drafts (
    id                      UUID NOT NULL,
    document_type_id        VARCHAR(64) NOT NULL,
    display_values          JSONB NOT NULL DEFAULT '{}'::jsonb,
    display_title           VARCHAR(255) NOT NULL,
    status                  VARCHAR(32) NOT NULL CHECK (status IN ('draft', 'modified', 'unpublished')),
    content                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    relations               JSONB NOT NULL DEFAULT '{}'::jsonb,
    schema_version          INT NOT NULL DEFAULT 1,
    version                 INT NOT NULL DEFAULT 1,
    last_published_revision INT,
    created_at              TIMESTAMPTZ NOT NULL,
    created_by              VARCHAR(255) NOT NULL,
    updated_at              TIMESTAMPTZ NOT NULL,
    updated_by              VARCHAR(255) NOT NULL,
    PRIMARY KEY (document_type_id, id)
);

CREATE INDEX IF NOT EXISTS idx_document_drafts_type_updated
    ON document_drafts (document_type_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_document_drafts_author
    ON document_drafts (updated_by, updated_at DESC);
