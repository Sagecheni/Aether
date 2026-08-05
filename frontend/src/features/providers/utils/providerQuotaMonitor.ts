import type { ProviderWithEndpointsSummary } from '@/api/endpoints/types/provider'
import type { Sub2ApiRemoteQuotaGroup } from '@/api/providerOps'

export type ProviderQuotaMonitor =
  | {
      source: 'remote_subscription'
      state: 'limited'
      limitUsd: number
      usedUsd: number
      remainingUsd: number
    }
  | {
      source: 'remote_subscription'
      state: 'unlimited' | 'pending' | 'exhausted'
    }

function finiteNonNegative(value: number | undefined): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null
}

function remoteGroupIsUnlimited(group: Sub2ApiRemoteQuotaGroup | null): boolean {
  if (!group) return false
  return group.daily_limit_usd <= 0
    && group.weekly_limit_usd <= 0
    && group.monthly_limit_usd <= 0
}

export function providerQuotaMonitor(
  provider: ProviderWithEndpointsSummary,
  remoteGroup: Sub2ApiRemoteQuotaGroup | null = null,
): ProviderQuotaMonitor | null {
  const remoteSubscription = provider.ops_remote_quota_enabled === true
  if (!remoteSubscription) return null

  if (provider.billing_type === 'monthly_quota') {
    const limitUsd = finiteNonNegative(provider.monthly_quota_usd)
    const usedUsd = finiteNonNegative(provider.monthly_used_usd)
    if (limitUsd !== null && usedUsd !== null) {
      if (limitUsd === 0 || usedUsd >= limitUsd) {
        return { source: 'remote_subscription', state: 'exhausted' }
      }
      return {
        source: 'remote_subscription',
        state: 'limited',
        limitUsd,
        usedUsd,
        remainingUsd: limitUsd - usedUsd,
      }
    }
  }

  return remoteGroupIsUnlimited(remoteGroup)
    ? { source: 'remote_subscription', state: 'unlimited' }
    : { source: 'remote_subscription', state: 'pending' }
}
