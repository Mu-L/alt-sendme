#!/usr/bin/env node

import { execSync } from 'node:child_process'
import fs from 'node:fs'

const CONVENTIONAL =
	/^(?<type>[a-z]+)(?:\((?<scope>[^)]+)\))?!?:\s*(?<subject>.+)$/i

const FEATURE_TYPES = new Set(['feat', 'feature'])
const FIX_TYPES = new Set(['fix', 'hotfix'])
const IMPROVEMENT_TYPES = new Set(['refactor', 'perf', 'improvement'])
const SKIP_TYPES = new Set([
	'chore',
	'ci',
	'build',
	'test',
	'style',
	'bump',
	'debug',
	'docs',
	'documentation',
])

const RELEASE_PR_TITLE = /^release\s+\d+\.\d+\.\d+/i

// Normalize equivalent scopes so commits group together (stable, not per-release).
const SCOPE_ALIASES = {
	pair: 'pairing',
}

const PAIRING_KEYWORDS =
	/\b(pair(?:ing|ed)?|invite|paired devices?|pairing ticket)\b/i

const TYPE_PRIORITY = {
	feat: 0,
	feature: 0,
	fix: 1,
	hotfix: 1,
	perf: 2,
	refactor: 3,
	improvement: 3,
	other: 4,
}

// Optional git author name -> GitHub login mapping for contributors who do not use
// GitHub noreply emails. Add entries here when needed; not updated each release.
const AUTHOR_LOGIN_ALIASES = {
	noordeen: 'noordeen123',
	ananthakrishnan: 'ananthan199601',
	retengart: 'Retengart',
	rubén: 'Rubensei',
	ruben: 'Rubensei',
}

function sh(command) {
	return execSync(command, { encoding: 'utf8' }).trim()
}

function parseCommit(subject) {
	const match = subject.match(CONVENTIONAL)
	if (!match?.groups) {
		return { type: 'other', scope: null, subject }
	}

	return {
		type: match.groups.type.toLowerCase(),
		scope: match.groups.scope?.toLowerCase() ?? null,
		subject: match.groups.subject.trim(),
	}
}

function capitalize(text) {
	if (!text) return text
	return text.charAt(0).toUpperCase() + text.slice(1)
}

function formatScopeLabel(scope) {
	if (!scope) return null
	if (scope === 'ui') return 'UI'
	if (scope === 'dpi') return 'DPI'
	return scope.replace(/-/g, ' ')
}

function bullet(text) {
	return `- ${text}`
}

function groupByScope(commits) {
	const groups = new Map()

	for (const commit of commits) {
		const key = commit.scope ?? '_general'
		if (!groups.has(key)) groups.set(key, [])
		groups.get(key).push(commit)
	}

	return groups
}

function normalizeScope(scope) {
	if (!scope) return null
	return SCOPE_ALIASES[scope] ?? scope
}

function normalizeCommit(commit) {
	const scope = normalizeScope(commit.scope)
	let subject = commit.subject

	if (/^prototype for pair mode$/i.test(subject)) {
		return null
	}

	if (!scope && FEATURE_TYPES.has(commit.type) && PAIRING_KEYWORDS.test(subject)) {
		return { ...commit, scope: 'pairing', subject }
	}

	return { ...commit, scope, subject }
}

function uniqueSubjects(commits) {
	const seen = new Set()
	const unique = []

	for (const commit of commits) {
		const key = commit.subject.toLowerCase()
		if (seen.has(key)) continue
		seen.add(key)
		unique.push(commit)
	}

	return unique
}

function pickRepresentativeSubjects(commits, limit = 2) {
	return [...uniqueSubjects(commits)]
		.sort((a, b) => {
			const priorityA = TYPE_PRIORITY[a.type] ?? TYPE_PRIORITY.other
			const priorityB = TYPE_PRIORITY[b.type] ?? TYPE_PRIORITY.other
			if (priorityA !== priorityB) return priorityA - priorityB
			return b.subject.length - a.subject.length
		})
		.slice(0, limit)
		.map((commit) => capitalize(commit.subject))
}

function summarizeScopeCommits(scope, commits) {
	const normalized = uniqueSubjects(commits)
	const scopeKey = scope === '_general' ? null : scope
	const label = formatScopeLabel(scopeKey)
	const subjects = pickRepresentativeSubjects(normalized, 2)
	const extraCount = Math.max(normalized.length - subjects.length, 0)

	if (!label) {
		if (subjects.length === 1 && extraCount === 0) return subjects[0]
		if (subjects.length === 1) {
			return `${subjects[0]} (+${extraCount} related changes)`
		}
		let text = subjects.join('; ')
		if (extraCount > 0) text += ` (+${extraCount} more)`
		return text
	}

	let text = `**${capitalize(label)}** — ${subjects[0]}`
	if (subjects.length > 1) {
		text += `; ${subjects[1]}`
	}
	if (extraCount > 0) {
		text += ` (+${extraCount} more)`
	}

	return text
}

function renderSection(title, commits) {
	if (commits.length === 0) return []

	const lines = [`## ${title}`, '']
	const groups = groupByScope(commits)

	for (const [scope, scopeCommits] of groups) {
		lines.push(bullet(summarizeScopeCommits(scope, scopeCommits)))
	}

	lines.push('')
	return lines
}

function pickHighlights(featureCommits) {
	const groups = groupByScope(featureCommits)
	const highlights = []
	const hasDevice = groups.has('device')
	const hasPairing = groups.has('pairing')

	for (const [scope, commits] of groups) {
		if (scope === 'identity' && hasDevice) continue
		if (scope === 'pair' && hasPairing) continue

		highlights.push({
			scope,
			weight: commits.length,
			text: summarizeScopeCommits(scope, commits),
		})
	}

	return highlights
		.sort((a, b) => b.weight - a.weight)
		.slice(0, 6)
		.map((entry) => entry.text)
}

function getCommitRange(currentTag, previousTag) {
	if (!previousTag) return 'HEAD'
	return `${previousTag}..HEAD`
}

function getPreviousTag(currentTag) {
	const tags = sh('git tag --sort=-v:refname')
		.split('\n')
		.filter(Boolean)

	return tags.find((tag) => tag !== currentTag) ?? ''
}

function loadCommitAuthors(commitRange) {
	const raw = sh(
		`git log ${commitRange} --pretty=format:'%an|%ae|%s' --no-merges`
	)

	if (!raw) return []

	return raw.split('\n').filter(Boolean).map((line) => {
		const [name, email, subject] = line.split('|')
		const parsed = parseCommit(subject)
		return {
			name,
			email,
			subject,
			...parsed,
		}
	})
}

function resolveGithubLogin(name, email) {
	const noreplyMatch = email.match(/^(?:\d+\+)?([^@]+)@users\.noreply\.github\.com$/i)
	if (noreplyMatch) return noreplyMatch[1]

	const alias = AUTHOR_LOGIN_ALIASES[name.trim().toLowerCase()]
	if (alias) return alias

	return null
}

function authorHasPriorContributions(name, email, previousTag) {
	if (!previousTag) return false

	try {
		const history = sh(`git log ${previousTag} --pretty=format:'%an|%ae' --no-merges`)
		if (!history) return false

		const login = resolveGithubLogin(name, email)
		return history.split('\n').some((line) => {
			const [priorName, priorEmail] = line.split('|')
			if (priorEmail === email) return true
			if (login && resolveGithubLogin(priorName, priorEmail) === login) return true
			return priorName.trim().toLowerCase() === name.trim().toLowerCase()
		})
	} catch {
		return false
	}
}

function describeContributorWork(commits) {
	const areas = new Set()

	for (const commit of commits) {
		if (FIX_TYPES.has(commit.type)) {
			areas.add(
				commit.scope
					? `${formatScopeLabel(commit.scope)} fixes`
					: 'bug fixes'
			)
			continue
		}

		if (FEATURE_TYPES.has(commit.type)) {
			areas.add(
				commit.scope
					? `${formatScopeLabel(commit.scope)} features`
					: 'new features'
			)
			continue
		}

		if (/test|e2e|coverage/i.test(commit.subject) || commit.type === 'test') {
			areas.add('test coverage')
			continue
		}

		if (commit.scope) {
			areas.add(`${formatScopeLabel(commit.scope)} improvements`)
		}
	}

	return [...areas].slice(0, 2).join(' and ') || 'their contributions'
}

function buildContributorCredit(contributors) {
	if (contributors.length === 0) return null

	const parts = contributors.map(
		({ login, work }) => `@${login} for ${work}`
	)

	if (parts.length === 1) {
		return `Thanking ${parts[0]}.`
	}

	return `Thanking ${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}.`
}

function collectContributors(commitRange, previousTag, repository, githubBody) {
	const owner = repository.split('/')[0]?.toLowerCase()
	const entries = new Map()

	for (const commit of loadCommitAuthors(commitRange)) {
		const login = resolveGithubLogin(commit.name, commit.email)
		if (!login) continue
		if (login.toLowerCase() === owner) continue

		if (!entries.has(login)) {
			entries.set(login, { login, commits: [], isNew: false })
		}

		entries.get(login).commits.push(commit)
	}

	for (const line of githubBody?.split('\n') ?? []) {
		const match = line.match(/\* .+ by @([A-Za-z0-9-]+) in /)
		if (!match) continue

		const login = match[1]
		if (login.toLowerCase() === owner) continue
		if (!entries.has(login)) {
			entries.set(login, { login, commits: [], isNew: false })
		}
	}

	const contributors = [...entries.values()].map((entry) => ({
		login: entry.login,
		work: describeContributorWork(entry.commits),
		isNew:
			entry.commits.length > 0 &&
			!authorHasPriorContributions(
				entry.commits[0].name,
				entry.commits[0].email,
				previousTag
			),
	}))

	contributors.sort((a, b) => a.login.localeCompare(b.login))

	return {
		all: contributors,
		newContributors: contributors.filter((contributor) => contributor.isNew),
	}
}

function renderContributorSections(contributorData, githubNewContributors) {
	const lines = []
	const seen = new Set(
		githubNewContributors.map((line) => {
			const match = line.match(/@([A-Za-z0-9-]+)/)
			return match?.[1] ?? ''
		})
	)

	const githubOnlyNew = githubNewContributors.map((line) =>
		line.replace(/^\* /, '- ')
	)

	const derivedNew = contributorData.newContributors
		.filter((contributor) => !seen.has(contributor.login))
		.map(
			(contributor) =>
				`- @${contributor.login} made their first contribution in this release`
		)

	const newContributorLines = [...githubOnlyNew, ...derivedNew]
	if (newContributorLines.length > 0) {
		lines.push('## New contributors', '')
		lines.push(...newContributorLines)
		lines.push('')
	}

	const thanks = buildContributorCredit(contributorData.all)
	if (thanks) {
		lines.push(thanks, '')
	}

	return lines
}

function loadCommits(commitRange) {
	const raw = sh(
		`git log ${commitRange} --pretty=format:%s --no-merges`
	)

	if (!raw) return []

	return raw
		.split('\n')
		.filter(Boolean)
		.map((subject) => ({ subject, ...parseCommit(subject) }))
		.filter((commit) => !SKIP_TYPES.has(commit.type))
		.map(normalizeCommit)
		.filter(Boolean)
}

function tryGithubGeneratedNotes(currentTag, previousTag, repository) {
	if (!process.env.GITHUB_TOKEN && !process.env.GH_TOKEN) {
		return null
	}

	try {
		const args = [
			'gh',
			'api',
			`repos/${repository}/releases/generate-notes`,
			'-f',
			`tag_name=${currentTag}`,
			'-f',
			'target_commitish=HEAD',
		]

		if (previousTag) {
			args.push('-f', `previous_tag_name=${previousTag}`)
		}

		const response = execSync(args.join(' '), { encoding: 'utf8' }).trim()
		return JSON.parse(response).body
	} catch {
		return null
	}
}

function extractGithubSections(body) {
	if (!body) {
		return { prLines: [], contributors: [] }
	}

	const lines = body.split('\n')
	const prLines = []
	const contributors = []
	let inContributors = false

	for (const line of lines) {
		if (line.startsWith('## New Contributors')) {
			inContributors = true
			continue
		}

		if (line.startsWith('## ') && inContributors) {
			inContributors = false
		}

		if (line.startsWith('**Full Changelog**')) {
			continue
		}

		if (inContributors) {
			if (line.startsWith('* ')) contributors.push(line)
			continue
		}

		if (!line.startsWith('* ')) continue

		const titleMatch = line.match(/\* (.+?) by @/i)
		const title = titleMatch?.[1] ?? ''
		if (RELEASE_PR_TITLE.test(title)) continue
		if (/^(chore|ci|build|test|bump|debug|docs|documentation)\b/i.test(title)) {
			continue
		}

		prLines.push(line.replace(/^\* /, '- '))
	}

	return { prLines, contributors }
}

function buildReleaseNotes({
	currentTag,
	previousTag,
	repository,
	githubBody,
	commits,
	commitRange,
}) {
	const features = commits.filter((commit) => FEATURE_TYPES.has(commit.type))
	const fixes = commits.filter((commit) => FIX_TYPES.has(commit.type))
	const improvements = commits.filter((commit) =>
		IMPROVEMENT_TYPES.has(commit.type)
	)

	const { prLines, contributors: githubNewContributors } =
		extractGithubSections(githubBody)
	const contributorData = collectContributors(
		commitRange,
		previousTag,
		repository,
		githubBody
	)
	const highlights = pickHighlights(features)

	const lines = []

	if (highlights.length > 0) {
		lines.push('## Release highlights', '')
		for (const highlight of highlights) {
			lines.push(bullet(highlight))
		}
		lines.push('')
	}

	if (prLines.length > 0) {
		lines.push("## What's changed", '')
		lines.push(...prLines)
		lines.push('')
	}

	lines.push(...renderSection('Bug fixes', fixes))

	if (improvements.length > 0 && improvements.length <= 8) {
		lines.push(...renderSection('Improvements', improvements))
	} else if (improvements.length > 8) {
		const groups = groupByScope(improvements)
		lines.push('## Improvements', '')
		lines.push(
			bullet(
				`${improvements.length} internal improvements across ${groups.size} areas`
			)
		)
		lines.push('')
	}

	lines.push(
		...renderContributorSections(contributorData, githubNewContributors)
	)

	if (previousTag) {
		lines.push(
			`**Full Changelog**: https://github.com/${repository}/compare/${previousTag}...${currentTag}`
		)
		lines.push('')
	}

	lines.push('---', '')
	lines.push('See the assets below to download and install this version.')

	return `${lines.join('\n').trim()}\n`
}

function writeGithubOutput(notes) {
	const outputPath = process.env.GITHUB_OUTPUT
	if (!outputPath) return

	fs.appendFileSync(outputPath, `notes<<EOF\n${notes}EOF\n`)
}

function main() {
	const currentTag =
		process.env.CURRENT_TAG ?? process.argv[2] ?? sh('git describe --tags --exact-match 2>/dev/null || true')
	const repository =
		process.env.GITHUB_REPOSITORY ??
		sh('git remote get-url origin')
			.replace(/\.git$/, '')
			.replace(/^git@github.com:/, '')
			.replace(/^https:\/\/github.com\//, '')

	if (!currentTag) {
		console.error('Error: CURRENT_TAG is required')
		process.exit(1)
	}

	try {
		sh('git fetch --tags --force --prune origin')
	} catch {
		// Local runs may not have remotes; continue with local tags.
	}

	const previousTag =
		process.env.PREVIOUS_TAG ?? getPreviousTag(currentTag)
	const commitRange = getCommitRange(currentTag, previousTag)
	const commits = loadCommits(commitRange)
	const githubBody = tryGithubGeneratedNotes(
		currentTag,
		previousTag,
		repository
	)

	const notes = buildReleaseNotes({
		currentTag,
		previousTag,
		repository,
		githubBody,
		commits,
		commitRange,
	})

	const outputFile = process.env.RELEASE_NOTES_FILE ?? 'release_notes.txt'
	fs.writeFileSync(outputFile, notes)

	if (process.env.GITHUB_OUTPUT) {
		writeGithubOutput(notes)
	}

	if (process.argv.includes('--print')) {
		process.stdout.write(notes)
	}
}

main()
