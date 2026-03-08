import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { NodePalette } from '../NodePalette';

describe('NodePalette', () => {
  it('renders all node type labels', () => {
    render(<NodePalette />);

    expect(screen.getByText('Trigger')).toBeInTheDocument();
    expect(screen.getByText('Agent')).toBeInTheDocument();
    expect(screen.getByText('Tool')).toBeInTheDocument();
    expect(screen.getByText('Condition')).toBeInTheDocument();
    expect(screen.getByText('Transform')).toBeInTheDocument();
    expect(screen.getByText('Human')).toBeInTheDocument();
    expect(screen.getByText('A2A')).toBeInTheDocument();
  });

  it('renders the Nodes heading', () => {
    render(<NodePalette />);
    expect(screen.getByText('Nodes')).toBeInTheDocument();
  });

  it('sets drag data with correct node kind on dragStart', () => {
    render(<NodePalette />);

    const triggerItem = screen.getByText('Trigger').closest('[draggable]');
    expect(triggerItem).toBeTruthy();

    const setData = vi.fn();
    fireEvent.dragStart(triggerItem!, {
      dataTransfer: { setData, effectAllowed: '' },
    });

    expect(setData).toHaveBeenCalledWith('application/reactflow-kind', 'trigger');
  });

  it('renders 7 draggable items', () => {
    const { container } = render(<NodePalette />);
    const draggables = container.querySelectorAll('[draggable="true"]');
    expect(draggables.length).toBe(7);
  });
});
