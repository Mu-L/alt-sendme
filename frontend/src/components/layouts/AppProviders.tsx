import { TranslationProvider } from '@/i18n'
import { IS_WEB } from '@/lib/platform'
import { WEB_APP_PORTAL_ID } from '@/lib/platformStyles'
import { AppThemeProvider } from '../AppThemeProvider'
import { AnchoredToastProvider, ToastProvider } from '../ui/toast'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

const queryClient = new QueryClient()

export function AppProviders({ children }: { children: React.ReactNode }) {
	const content = (
		<TranslationProvider>
			<QueryClientProvider client={queryClient}>
				<ToastProvider position="bottom-center" limit={1}>
					<AnchoredToastProvider>
						<AppThemeProvider>{children}</AppThemeProvider>
					</AnchoredToastProvider>
				</ToastProvider>
			</QueryClientProvider>
		</TranslationProvider>
	)

	if (!IS_WEB) {
		return content
	}

	return (
		<div className="relative h-full w-full min-h-0">
			{content}
			<div
				id={WEB_APP_PORTAL_ID}
				className="pointer-events-none absolute inset-0 z-[100] [&>*]:pointer-events-auto"
			/>
		</div>
	)
}
