import type { ReactElement, ReactNode } from 'react';
import { Button, Group, Modal, Stack, Text } from '@mantine/core';
import { modals } from '@mantine/modals';
import i18n from '@/i18n/config';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export interface ConfirmDialogOptions {
  title?: ReactNode;
  children?: ReactNode;
  message?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel?: () => void;
}

export interface ConfirmModalProps {
  opened: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title?: ReactNode;
  children?: ReactNode;
  message?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  loading?: boolean;
}

// ---------------------------------------------------------------------------
// Primary Exports
// ---------------------------------------------------------------------------

/**
 * Imperative modal trigger wrapping Mantine's modals.openConfirmModal.
 */
export function openConfirmDialog(options: ConfirmDialogOptions): void {
  const confirmText = options.confirmLabel ?? i18n.t('common.confirm', 'Confirm');
  const cancelText = options.cancelLabel ?? i18n.t('common.cancel', 'Cancel');
  const content = options.children ?? (options.message ? <Text size="sm">{options.message}</Text> : null);

  modals.openConfirmModal({
    title: options.title,
    children: content,
    labels: { confirm: confirmText, cancel: cancelText },
    confirmProps: { color: options.danger ? 'red' : 'blue' },
    onConfirm: options.onConfirm,
    onCancel: options.onCancel,
  });
}

/**
 * Controlled confirm modal component.
 */
export function ConfirmModal({
  opened,
  onClose,
  onConfirm,
  title,
  children,
  message,
  confirmLabel,
  cancelLabel,
  danger,
  loading,
}: ConfirmModalProps): ReactElement {
  const confirmText = confirmLabel ?? i18n.t('common.confirm', 'Confirm');
  const cancelText = cancelLabel ?? i18n.t('common.cancel', 'Cancel');
  const content = children ?? (message ? <Text size="sm">{message}</Text> : null);

  return (
    <Modal opened={opened} onClose={onClose} title={title} centered>
      <Stack gap="md">
        {content}
        <Group justify="flex-end" gap="sm">
          <Button variant="default" onClick={onClose} disabled={loading}>
            {cancelText}
          </Button>
          <Button
            color={danger ? 'red' : 'blue'}
            onClick={onConfirm}
            loading={loading}
          >
            {confirmText}
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
