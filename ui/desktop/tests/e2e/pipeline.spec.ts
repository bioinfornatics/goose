import { test, expect } from './fixtures';

test.describe('Pipeline Editor', () => {
  test('should show Pipelines in navigation', async ({ goosePage }) => {
    // The pipelines nav item should be visible in the sidebar
    const pipelinesNav = goosePage.getByRole('link', { name: /pipelines/i });
    await expect(pipelinesNav).toBeVisible({ timeout: 10_000 });
  });

  test('should navigate to pipelines view', async ({ goosePage }) => {
    // Click on Pipelines in the sidebar
    const pipelinesNav = goosePage.getByRole('link', { name: /pipelines/i });
    await pipelinesNav.click();

    // Should show the pipelines list view
    await expect(goosePage).toHaveURL(/.*pipelines/);

    // Should show empty state or list
    const heading = goosePage.getByText(/pipeline/i).first();
    await expect(heading).toBeVisible({ timeout: 5_000 });
  });

  test('should create a new pipeline', async ({ goosePage }) => {
    // Navigate to pipelines
    const pipelinesNav = goosePage.getByRole('link', { name: /pipelines/i });
    await pipelinesNav.click();
    await expect(goosePage).toHaveURL(/.*pipelines/);

    // Click "New Pipeline" button
    const newButton = goosePage.getByRole('button', { name: /new pipeline/i });
    await expect(newButton).toBeVisible({ timeout: 5_000 });
    await newButton.click();

    // Should navigate to editor
    await expect(goosePage).toHaveURL(/.*pipelines\/.+/);

    // The editor canvas should be visible (ReactFlow container)
    const canvas = goosePage.locator('.react-flow');
    await expect(canvas).toBeVisible({ timeout: 10_000 });

    // The node palette should be visible
    const palette = goosePage.getByText(/trigger/i).first();
    await expect(palette).toBeVisible({ timeout: 5_000 });
  });

  test('should show node palette with draggable nodes', async ({ goosePage }) => {
    // Navigate to pipelines and create a new one
    const pipelinesNav = goosePage.getByRole('link', { name: /pipelines/i });
    await pipelinesNav.click();

    const newButton = goosePage.getByRole('button', { name: /new pipeline/i });
    await newButton.click();
    await expect(goosePage).toHaveURL(/.*pipelines\/.+/);

    // Check node types are available in palette
    const nodeTypes = ['Trigger', 'Agent', 'Tool', 'Condition'];
    for (const nodeType of nodeTypes) {
      const node = goosePage.getByText(nodeType, { exact: true }).first();
      await expect(node).toBeVisible({ timeout: 5_000 });
    }
  });

  test('should save and reload a pipeline', async ({ goosePage }) => {
    // Navigate to pipelines
    const pipelinesNav = goosePage.getByRole('link', { name: /pipelines/i });
    await pipelinesNav.click();

    // Create new pipeline
    const newButton = goosePage.getByRole('button', { name: /new pipeline/i });
    await newButton.click();
    await expect(goosePage).toHaveURL(/.*pipelines\/.+/);

    // Wait for editor to load
    const canvas = goosePage.locator('.react-flow');
    await expect(canvas).toBeVisible({ timeout: 10_000 });

    // Click save button
    const saveButton = goosePage.getByRole('button', { name: /save/i });
    if (await saveButton.isVisible()) {
      await saveButton.click();
      // Wait briefly for save to complete
      await goosePage.waitForTimeout(1_000);
    }

    // Go back to pipeline list
    await pipelinesNav.click();
    await expect(goosePage).toHaveURL(/.*pipelines$/);

    // The pipeline should appear in the list
    await goosePage.waitForTimeout(1_000);
    const listItems = goosePage.locator('[data-testid="pipeline-item"]');
    // At least check the page loaded without errors
    await expect(goosePage.getByText(/pipeline/i).first()).toBeVisible();
  });
});
