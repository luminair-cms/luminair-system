import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ConfirmModal, openConfirmDialog } from '@/components/common/ConfirmModal';
import { renderWithProviders } from '@/test-utils/render';

describe('ConfirmModal', () => {
  it('renders controlled modal content and handles confirm and cancel', async () => {
    const user = userEvent.setup();
    const handleClose = vi.fn();
    const handleConfirm = vi.fn();

    renderWithProviders(
      <ConfirmModal
        opened={true}
        onClose={handleClose}
        onConfirm={handleConfirm}
        title="Delete Item"
        message="Are you sure you want to delete this item?"
        confirmLabel="Yes, Delete"
        cancelLabel="No, Cancel"
        danger
      />
    );

    expect(screen.getByText('Delete Item')).toBeInTheDocument();
    expect(screen.getByText('Are you sure you want to delete this item?')).toBeInTheDocument();

    const cancelButton = screen.getByRole('button', { name: 'No, Cancel' });
    await user.click(cancelButton);
    expect(handleClose).toHaveBeenCalledTimes(1);

    const confirmButton = screen.getByRole('button', { name: 'Yes, Delete' });
    await user.click(confirmButton);
    expect(handleConfirm).toHaveBeenCalledTimes(1);
  });

  it('triggers imperative confirm dialog with modals provider', async () => {
    const user = userEvent.setup();
    const handleConfirm = vi.fn();

    function TriggerComponent() {
      return (
        <button
          onClick={() =>
            openConfirmDialog({
              title: 'Confirm Action',
              message: 'Proceed with changes?',
              onConfirm: handleConfirm,
            })
          }
        >
          Open Modal
        </button>
      );
    }

    renderWithProviders(<TriggerComponent />);

    await user.click(screen.getByRole('button', { name: 'Open Modal' }));

    expect(screen.getByText('Confirm Action')).toBeInTheDocument();
    expect(screen.getByText('Proceed with changes?')).toBeInTheDocument();

    const confirmBtn = screen.getByRole('button', { name: /confirm/i });
    await user.click(confirmBtn);
    expect(handleConfirm).toHaveBeenCalledTimes(1);
  });
});
