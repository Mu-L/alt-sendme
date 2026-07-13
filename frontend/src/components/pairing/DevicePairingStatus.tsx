import { useTranslation } from '../../i18n/react-i18next-compat'
import type { PairedDevice } from '@/lib/pairing-api'
import { cn } from '@/lib/utils'

type StatusNamespace = 'sender' | 'settings'

interface DevicePairingStatusProps {
	device: Pick<PairedDevice, 'online' | 'pairing_status'>
	namespace: StatusNamespace
	className?: string
}

export function DevicePairingStatus({
	device,
	namespace,
	className,
}: DevicePairingStatusProps) {
	const { t } = useTranslation()
	const prefix =
		namespace === 'sender'
			? 'common:sender.pairedDevices'
			: 'common:settings.devices'

	if (device.pairing_status === 'unpaired-remotely') {
		return (
			<span
				className={cn(
					'inline-flex items-center gap-1.5 text-xs text-amber-600 dark:text-amber-500',
					className
				)}
			>
				<span className="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500" />
				{t(`${prefix}.statusUnpaired`)}
			</span>
		)
	}

	if (device.pairing_status === 'stale-local-identity') {
		return (
			<span
				className={cn(
					'inline-flex items-center gap-1.5 text-xs text-amber-600 dark:text-amber-500',
					className
				)}
			>
				<span className="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500" />
				{t(`${prefix}.statusRePairRequired`)}
			</span>
		)
	}

	if (device.online) {
		return (
			<span
				className={cn(
					'inline-flex items-center gap-1.5 text-xs text-muted-foreground',
					className
				)}
			>
				<span className="h-1.5 w-1.5 shrink-0 rounded-full bg-green-500" />
				{t(`${prefix}.statusOnline`)}
			</span>
		)
	}

	return (
		<span
			className={cn(
				'inline-flex items-center gap-1.5 text-xs text-muted-foreground',
				className
			)}
		>
			<span className="h-1.5 w-1.5 shrink-0 rounded-full bg-muted-foreground/40" />
			{t(`${prefix}.statusOffline`)}
		</span>
	)
}
