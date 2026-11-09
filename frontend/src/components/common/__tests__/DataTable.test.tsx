import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { DataTable, type DataTableColumn } from '@/components/common/DataTable';
import { renderWithProviders } from '@/test-utils/render';

interface TestUser {
  id: string;
  name: string;
  role: string;
}

describe('DataTable', () => {
  const columns: DataTableColumn<TestUser>[] = [
    { key: 'name', header: 'Name' },
    { key: 'role', header: 'Role' },
  ];

  const sampleData: TestUser[] = [
    { id: '1', name: 'Alice', role: 'Admin' },
    { id: '2', name: 'Bob', role: 'Editor' },
  ];

  it('renders table headers and data rows correctly', () => {
    renderWithProviders(
      <DataTable
        columns={columns}
        data={sampleData}
        keyExtractor={(u) => u.id}
      />
    );

    expect(screen.getByText('Name')).toBeInTheDocument();
    expect(screen.getByText('Role')).toBeInTheDocument();
    expect(screen.getByText('Alice')).toBeInTheDocument();
    expect(screen.getByText('Admin')).toBeInTheDocument();
    expect(screen.getByText('Bob')).toBeInTheDocument();
    expect(screen.getByText('Editor')).toBeInTheDocument();
  });

  it('supports custom column render callbacks', () => {
    const customColumns: DataTableColumn<TestUser>[] = [
      {
        key: 'name',
        header: 'Name',
        render: (u) => <strong>Custom {u.name}</strong>,
      },
    ];

    renderWithProviders(
      <DataTable
        columns={customColumns}
        data={[sampleData[0]]}
        keyExtractor={(u) => u.id}
      />
    );

    expect(screen.getByText('Custom Alice')).toBeInTheDocument();
  });

  it('renders empty state message when data array is empty', () => {
    renderWithProviders(
      <DataTable
        columns={columns}
        data={[]}
        keyExtractor={(u) => u.id}
        emptyMessage="No users found."
      />
    );

    expect(screen.getByText('No users found.')).toBeInTheDocument();
  });

  it('renders loading state without displaying empty message', () => {
    renderWithProviders(
      <DataTable
        columns={columns}
        data={[]}
        keyExtractor={(u) => u.id}
        loading={true}
        emptyMessage="No users found."
      />
    );

    expect(screen.queryByText('No users found.')).not.toBeInTheDocument();
    // Loader is rendered within the table body
    expect(document.querySelector('.mantine-Loader-root')).toBeInTheDocument();
  });

  it('renders pagination controls and triggers onPageChange callback', async () => {
    const user = userEvent.setup();
    const handlePageChange = vi.fn();

    renderWithProviders(
      <DataTable
        columns={columns}
        data={sampleData}
        keyExtractor={(u) => u.id}
        page={1}
        totalPages={3}
        onPageChange={handlePageChange}
      />
    );

    // Page 2 button
    const page2Button = screen.getByRole('button', { name: '2' });
    expect(page2Button).toBeInTheDocument();

    await user.click(page2Button);
    expect(handlePageChange).toHaveBeenCalledWith(2);
  });
});
