import { test, expect } from '@playwright/test'
import { installMockBackend } from '../support/mock-backend'

test('compute stays unavailable until verified and removal waits for confirmation', async ({
  page,
}) => {
  await installMockBackend(page)
  await page.addInitScript(() => {
    const original = window.fetch
    let stage = 'idle'
    Object.assign(window, {
      setComputeStage: (next: string) => {
        stage = next
      },
    })
    window.fetch = async (input, init) => {
      const path = String(input)
      if (path === '/app/compute')
        return Response.json({
          enabled: true,
          stage,
          cap_micro_usd: 1_200_000,
          node:
            stage === 'idle' || stage === 'removed'
              ? null
              : { node_id: 'synthetic-owner', peers: [] },
          expires_at: null,
        })
      if (path === '/app/compute/up' || path === '/app/compute/down') {
        if (new Headers(init?.headers).get('x-m8s-desktop') !== '1')
          return new Response('', { status: 403 })
        stage = path.endsWith('/up') ? 'provisioning' : 'removing'
        return Response.json({ accepted: true })
      }
      return original(input, init)
    }
  })
  await page.goto('/')
  await expect(page.getByTestId('add-compute')).toBeVisible()
  await page.screenshot({ path: test.info().outputPath('compute-idle.png') })
  await page.getByTestId('add-compute').click()
  await expect(page.getByRole('heading', { name: 'Starting your GPU' })).toBeVisible()
  await expect(page.getByTestId('add-compute')).toHaveCount(0)
  await page.evaluate(() =>
    (window as unknown as { setComputeStage: (s: string) => void }).setComputeStage('ready'),
  )
  await expect(page.getByRole('heading', { name: 'Qwen 27B is ready' })).toBeVisible()
  await expect(page.getByTestId('mesh-panel')).toBeVisible()
  await page.screenshot({ path: test.info().outputPath('compute-ready-synthetic.png') })
  await page.getByTestId('remove-compute').click()
  await expect(
    page.getByRole('heading', { name: /waiting for provider confirmation/ }),
  ).toBeVisible()
  await expect(page.getByRole('heading', { name: /provider confirmed$/ })).toHaveCount(0)
  await page.evaluate(() =>
    (window as unknown as { setComputeStage: (s: string) => void }).setComputeStage('removed'),
  )
  await expect(
    page.getByRole('heading', { name: 'GPU removed · provider confirmed' }),
  ).toBeVisible()
  await expect(page.getByTestId('add-compute')).toHaveCount(0)
})
