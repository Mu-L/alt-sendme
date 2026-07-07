import { getCurrentWindow, invoke, openDialog } from '@/lib/platform-api'
import { processWebDataTransfer } from '@/lib/web-drag-drop'
import { selectSendDocument, selectSendFolder } from '@/plugins/nativeUtils'
import { useCallback, useEffect, useRef, useState, type DragEvent } from 'react'
import { useTranslation } from '../i18n/react-i18next-compat'
import type { AlertDialogState, AlertType } from '../types/ui'
import { IS_ANDROID, IS_TAURI, IS_WEB } from '@/lib/platform'

export interface UseDragDropReturn {
	isDragActive: boolean
	pathType: 'file' | 'directory' | null
	showFullPath: boolean
	alertDialog: AlertDialogState

	// Android SAF copy progress (runtime-only, never persisted)
	isCopying: boolean
	copyProgress: number
	copyFileName: string
	copyTotalBytes: string
	cancelCopy: () => Promise<void>

	toggleFullPath: () => void
	browseFile: () => Promise<void>
	addMoreFiles: () => Promise<void>
	addMoreFolders: () => Promise<void>
	browseFolder: () => Promise<void>
	showAlert: (title: string, description: string, type?: AlertType) => void
	closeAlert: () => void
	checkPathType: (
		path: string,
		pathType?: 'file' | 'directory'
	) => Promise<void>
	dropzoneDragProps?: {
		onDragEnter: (event: DragEvent<HTMLElement>) => void
		onDragOver: (event: DragEvent<HTMLElement>) => void
		onDragLeave: (event: DragEvent<HTMLElement>) => void
		onDrop: (event: DragEvent<HTMLElement>) => void
	}
}

export function useDragDrop(
	onFileSelect: (
		path: string,
		pathType?: 'file' | 'directory'
	) => void | Promise<void>,
	onFilesSelect?: (
		paths: string[],
		pathType?: 'file' | 'directory'
	) => void | Promise<void>
): UseDragDropReturn {
	const { t } = useTranslation()
	const [isDragActive, setIsDragActive] = useState(false)
	const [pathType, setPathType] = useState<'file' | 'directory' | null>(null)
	const [showFullPath, setShowFullPath] = useState(false)
	const [alertDialog, setAlertDialog] = useState<AlertDialogState>({
		isOpen: false,
		title: '',
		description: '',
		type: 'info',
	})

	// Android SAF copy progress (runtime-only, never persisted)
	const [isCopying, setIsCopying] = useState(false)
	const [copyProgress, setCopyProgress] = useState(0)
	const [copyFileName, setCopyFileName] = useState('')
	const [copyTotalBytes, setCopyTotalBytes] = useState('0')
	const cancelRef = useRef<(() => Promise<void>) | null>(null)
	const webDragDepthRef = useRef(0)

	const showAlert = useCallback(
		(title: string, description: string, type: AlertType = 'info') => {
			setAlertDialog({ isOpen: true, title, description, type })
		},
		[]
	)

	const closeAlert = useCallback(() => {
		setAlertDialog((prev) => ({ ...prev, isOpen: false }))
	}, [])

	const toggleFullPath = useCallback(() => {
		setShowFullPath((prev) => !prev)
	}, [])

	const checkPathType = useCallback(
		async (path: string, pathType?: 'file' | 'directory') => {
			if (pathType) {
				setPathType(pathType)
				return
			}

			try {
				const type = await invoke<string>('check_path_type', { path })
				setPathType(type as 'file' | 'directory')
			} catch (error) {
				console.error('Failed to check path type:', error)
				setPathType(null)
			}
		},
		[]
	)

	const triggerFileSelect = useCallback(
		async (path: string, pathType?: 'file' | 'directory') => {
			try {
				await Promise.resolve(onFileSelect(path, pathType))
			} catch (error) {
				console.error('Failed to handle selected path:', error)
				showAlert(
					t('common:errors.fileDialogFailed'),
					`${t('common:errors.fileDialogFailedDesc')}: ${error}`,
					'error'
				)
			}
		},
		[onFileSelect, showAlert, t]
	)

	const triggerFilesSelect = useCallback(
		async (paths: string[], pathType?: 'file' | 'directory') => {
			if (!paths.length) {
				return
			}

			if (onFilesSelect) {
				try {
					await Promise.resolve(onFilesSelect(paths, pathType))
					return
				} catch (error) {
					console.error('Failed to handle selected paths:', error)
					showAlert(
						t('common:errors.fileDialogFailed'),
						`${t('common:errors.fileDialogFailedDesc')}: ${error}`,
						'error'
					)
					return
				}
			}

			for (const path of paths) {
				await triggerFileSelect(path, pathType)
			}
		},
		[onFilesSelect, showAlert, t, triggerFileSelect]
	)

	const cancelCopy = useCallback(async () => {
		try {
			await cancelRef.current?.()
		} finally {
			setIsCopying(false)
			setCopyProgress(0)
			setCopyFileName('')
			setCopyTotalBytes('0')
			cancelRef.current = null
		}
	}, [])

	const browseFile = useCallback(async () => {
		try {
			if (IS_ANDROID) {
				const handler = await selectSendDocument(
					(path, size) => {
						setCopyFileName(path.split(/[/\\]/).filter(Boolean).pop() || path)
						setCopyTotalBytes(size.toString())
						setCopyProgress(0)
						setIsCopying(true)
					},
					(event) => {
						setCopyProgress(event.progress)
					},
					async (path) => {
						setIsCopying(false)
						setCopyProgress(0)
						setCopyFileName('')
						setCopyTotalBytes('0')
						cancelRef.current = null
						await triggerFilesSelect([path], 'file')
					}
				)

				if (handler) {
					cancelRef.current = () => handler.cancelJob()
				}

				return
			} else {
				const selected = await openDialog({
					multiple: true,
					directory: false,
				})

				if (selected) {
					const paths = Array.isArray(selected) ? selected : [selected]
					await triggerFilesSelect(paths, 'file')
				}
			}
		} catch (error) {
			console.error('Failed to open file dialog:', error)
			showAlert(
				t('common:errors.fileDialogFailed'),
				`${t('common:errors.fileDialogFailedDesc')}: ${error}`,
				'error'
			)
		}
	}, [showAlert, t, triggerFilesSelect])

	const browseFolder = useCallback(async () => {
		try {
			if (IS_ANDROID) {
				const handler = await selectSendFolder(
					(path, size) => {
						setCopyFileName(path.split(/[/\\]/).filter(Boolean).pop() || path)
						setCopyTotalBytes(size.toString())
						setCopyProgress(0)
						setIsCopying(true)
					},
					(event) => {
						setCopyProgress(event.progress)
					},
					async (path) => {
						setIsCopying(false)
						setCopyProgress(0)
						setCopyFileName('')
						setCopyTotalBytes('0')
						cancelRef.current = null
						await triggerFilesSelect([path], 'directory')
					}
				)

				if (handler) {
					cancelRef.current = () => handler.cancelJob()
				}

				return
			} else {
				const selected = await openDialog({
					multiple: false,
					directory: true,
				})

				if (selected) {
					const path = Array.isArray(selected) ? selected[0] : selected
					if (path) {
						await triggerFilesSelect([path], 'directory')
					}
				}
			}
		} catch (error) {
			console.error('Failed to open folder dialog:', error)
			showAlert(
				t('common:errors.folderDialogFailed'),
				`${t('common:errors.folderDialogFailedDesc')}: ${error}`,
				'error'
			)
		}
	}, [showAlert, t, triggerFilesSelect])

	const handleWebDragEnter = useCallback((event: DragEvent<HTMLElement>) => {
		event.preventDefault()
		event.stopPropagation()
		webDragDepthRef.current += 1
		setIsDragActive(true)
	}, [])

	const handleWebDragOver = useCallback((event: DragEvent<HTMLElement>) => {
		event.preventDefault()
		event.stopPropagation()
		event.dataTransfer.dropEffect = 'copy'
	}, [])

	const handleWebDragLeave = useCallback((event: DragEvent<HTMLElement>) => {
		event.preventDefault()
		event.stopPropagation()
		webDragDepthRef.current = Math.max(0, webDragDepthRef.current - 1)
		if (webDragDepthRef.current === 0) {
			setIsDragActive(false)
		}
	}, [])

	const handleWebDrop = useCallback(
		async (event: DragEvent<HTMLElement>) => {
			event.preventDefault()
			event.stopPropagation()
			webDragDepthRef.current = 0
			setIsDragActive(false)

			try {
				const { paths, pathType } = await processWebDataTransfer(
					event.dataTransfer
				)
				if (paths.length > 0) {
					await triggerFilesSelect(paths, pathType)
				}
			} catch (error) {
				console.error('Failed to handle dropped files:', error)
				showAlert(
					t('common:errors.fileDialogFailed'),
					`${t('common:errors.fileDialogFailedDesc')}: ${error}`,
					'error'
				)
			}
		},
		[showAlert, t, triggerFilesSelect]
	)

	// Prevent the browser from opening/navigating to files dropped outside the dropzone.
	useEffect(() => {
		if (!IS_WEB) {
			return
		}

		const rejectWindowFileDrop = (event: globalThis.DragEvent) => {
			if (!event.dataTransfer?.types.includes('Files')) {
				return
			}
			event.preventDefault()
		}

		window.addEventListener('dragover', rejectWindowFileDrop, true)
		window.addEventListener('drop', rejectWindowFileDrop, true)

		return () => {
			window.removeEventListener('dragover', rejectWindowFileDrop, true)
			window.removeEventListener('drop', rejectWindowFileDrop, true)
		}
	}, [])

	useEffect(() => {
		if (!IS_TAURI) {
			return
		}

		let dropUnlisten: (() => void) | undefined
		let hoverUnlisten: (() => void) | undefined
		let cancelUnlisten: (() => void) | undefined
		let disposed = false

		const setupWindowListeners = async () => {
			const window = await getCurrentWindow()
			if (disposed) return

			window
				.listen<{ paths: string[]; position: { x: number; y: number } }>(
					'tauri://drag-drop',
					(event) => {
						setIsDragActive(false)

						if (event.payload?.paths && event.payload.paths.length > 0) {
							void triggerFilesSelect(event.payload.paths)
						}
					}
				)
				.then((unlisten) => {
					if (disposed) {
						unlisten()
						return
					}
					dropUnlisten = unlisten
				})
				.catch((err) => {
					console.error('Failed to register drag-drop listener:', err)
				})

			window
				.listen('tauri://drag-hover', () => {
					setIsDragActive(true)
				})
				.then((unlisten) => {
					if (disposed) {
						unlisten()
						return
					}
					hoverUnlisten = unlisten
				})
				.catch((err) => {
					console.error('Failed to register drag-hover listener:', err)
				})

			window
				.listen('tauri://drag-leave', () => {
					setIsDragActive(false)
				})
				.then((unlisten) => {
					if (disposed) {
						unlisten()
						return
					}
					cancelUnlisten = unlisten
				})
				.catch((err) => {
					console.error('Failed to register drag-leave listener:', err)
				})
		}

		void setupWindowListeners()

		return () => {
			disposed = true
			dropUnlisten?.()
			hoverUnlisten?.()
			cancelUnlisten?.()
		}
	}, [triggerFilesSelect])

	const addMoreFiles = useCallback(async () => {
		await browseFile()
	}, [browseFile])

	const addMoreFolders = useCallback(async () => {
		try {
			if (IS_ANDROID) {
				const handler = await selectSendFolder(
					(path, size) => {
						setCopyFileName(path.split(/[/\\]/).filter(Boolean).pop() || path)
						setCopyTotalBytes(size.toString())
						setCopyProgress(0)
						setIsCopying(true)
					},
					(event) => {
						setCopyProgress(event.progress)
					},
					async (path) => {
						setIsCopying(false)
						setCopyProgress(0)
						setCopyFileName('')
						setCopyTotalBytes('0')
						cancelRef.current = null
						await triggerFilesSelect([path], 'directory')
					}
				)

				if (handler) {
					cancelRef.current = () => handler.cancelJob()
				}

				return
			}

			const selected = await openDialog({
				multiple: true,
				directory: true,
			})

			if (!selected) return

			const paths = Array.isArray(selected) ? selected : [selected]
			await triggerFilesSelect(paths, 'directory')
		} catch (error) {
			console.error('Failed to open folders dialog:', error)
			showAlert(
				t('common:errors.folderDialogFailed'),
				`${t('common:errors.folderDialogFailedDesc')}: ${error}`,
				'error'
			)
		}
	}, [showAlert, t, triggerFilesSelect])

	return {
		isDragActive,
		pathType,
		showFullPath,
		alertDialog,

		isCopying,
		copyProgress,
		copyFileName,
		copyTotalBytes,
		cancelCopy,

		toggleFullPath,
		browseFile,
		addMoreFiles,
		addMoreFolders,
		browseFolder,
		showAlert,
		closeAlert,
		checkPathType,
		dropzoneDragProps: IS_WEB
			? {
					onDragEnter: handleWebDragEnter,
					onDragOver: handleWebDragOver,
					onDragLeave: handleWebDragLeave,
					onDrop: handleWebDrop,
				}
			: undefined,
	}
}
