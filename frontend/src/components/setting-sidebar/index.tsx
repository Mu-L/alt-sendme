import { useEffect } from 'react'
import { useLocation } from 'react-router-dom'
import {
	SettingSidebarContent,
	SettingSidebarCore,
	SettingSidebarFooter,
	SettingSidebarHeader,
	SettingSidebarRoot,
	SettingSidebarTitle,
} from './setting-sidebar'
import { SettingSidebarUpdateAlert } from './setting-sidebar-update-alert'
import { IS_WEB } from '@/lib/platform'
import { cn } from '@/lib/utils'
import { useSidebar } from '../ui/sidebar'

export default function SettingSidebar() {
	const { pathname } = useLocation()
	const { isMobile, setOpenMobile } = useSidebar()

	// On mobile, open the nav sheet only for the settings root so users land on
	// navigation. Deep links (e.g. Pair Device → /settings/devices) go straight
	// to that page with the sheet closed.
	useEffect(() => {
		if (isMobile && pathname === '/settings') {
			setOpenMobile(true)
		}
	}, [isMobile, pathname, setOpenMobile])

	return (
		<SettingSidebarRoot
			className={cn(IS_WEB ? 'h-full' : 'h-[calc(100svh)]')}
			variant="floating"
		>
			<SettingSidebarHeader className="border-b">
				<SettingSidebarTitle prev="/" />
			</SettingSidebarHeader>
			<SettingSidebarContent>
				<SettingSidebarCore />
			</SettingSidebarContent>
			<SettingSidebarUpdateAlert />
			<SettingSidebarFooter />
		</SettingSidebarRoot>
	)
}
