import { get, getEntries, getEach, makeContext, resolve } from '../../../rex-lang/rexc.ts'
import type { RxNode, RxObject, RxArray, RxChain, RxContext } from '../../../rex-lang/rexc.ts'
import type { RexcNode, RexcParser, KeyInfo } from './rexc-parser.ts'

function keyInfoFromRxNode(keyNode: RxNode, context?: RxContext): KeyInfo {
	if (keyNode.type === 'pointer') {
		let resolvedValue: string | undefined
		if (context) {
			try {
				const resolved = resolve(context, keyNode)
				if (typeof resolved === 'string') resolvedValue = resolved
			} catch { }
		}
		return { kind: 'pointer', offset: keyNode.right, targetOffset: keyNode.target as number, resolvedValue }
	}
	if (keyNode.type === 'chain') {
		let resolvedValue: string | undefined
		if (context) {
			try {
				const resolved = resolve(context, keyNode)
				if (typeof resolved === 'string') resolvedValue = resolved
			} catch { }
		}
		return { kind: 'chain', offset: keyNode.right, resolvedValue }
	}
	return { kind: 'plain' }
}

// Follow pointers (and chains of pointers) to find the ultimate target RxNode.
function followTarget(data: Uint8Array, offset: number, maxDepth = 20): RxNode {
	let node = get(data, offset)
	let depth = 0
	while (depth++ < maxDepth) {
		if (node.type === 'pointer' && typeof node.target === 'number') {
			node = get(data, node.target)
		} else {
			break
		}
	}
	return node
}

function rxNodeToRexcNode(node: RxNode, key?: string | number, keyInfo?: KeyInfo, context?: RxContext): RexcNode {
	const ki = keyInfo?.kind !== 'plain' ? keyInfo : undefined
	switch (node.type) {
		case 'primitive': {
			const v = node.value
			if (typeof v === 'string') return { kind: 'string', start: node.left, end: node.right, key: key as string, keyInfo: ki, value: v }
			if (typeof v === 'number') return { kind: 'number', start: node.left, end: node.right, key: key as string, keyInfo: ki, value: v }
			if (typeof v === 'boolean') return { kind: 'boolean', start: node.left, end: node.right, key: key as string, keyInfo: ki, value: v }
			if (v === null) return { kind: 'null', start: node.left, end: node.right, key: key as string, keyInfo: ki }
			return { kind: 'undefined', start: node.left, end: node.right, key: key as string, keyInfo: ki }
		}
		case 'pointer': {
			if (typeof node.target === 'string')
				return { kind: 'reference', start: node.left, end: node.right, key: key as string, keyInfo: ki, refId: node.target }
			const result: RexcNode = { kind: 'pointer', start: node.left, end: node.right, key: key as string, keyInfo: ki, targetOffset: node.target }
			if (context) {
				try {
					const target = get(context.data, node.target)
					if (target.type === 'primitive' || target.type === 'chain') {
						const resolved = resolve(context, target)
						if (typeof resolved === 'string') {
							result.resolvedValue = resolved
							result.resolvedKind = target.type === 'chain' ? 'chain' : 'string'
							if (target.type === 'chain') result.chainOffset = target.right
						} else if (typeof resolved === 'number') {
							result.resolvedValue = String(resolved)
							result.resolvedKind = 'number'
						} else if (typeof resolved === 'boolean' || resolved === null) {
							result.resolvedValue = String(resolved)
							result.resolvedKind = String(typeof resolved === 'boolean' ? 'boolean' : 'null')
						}
					} else if (target.type === 'object' || target.type === 'array') {
						result.resolvedKind = target.type
					} else if (target.type === 'pointer') {
						// Follow chain of pointers to find ultimate target
						const ultimate = followTarget(context.data, node.target)
						if (ultimate.type === 'object' || ultimate.type === 'array') {
							result.resolvedKind = ultimate.type
						}
					}
				} catch { }
			}
			return result
		}
		case 'object':
			return { kind: 'object', start: node.left, end: node.right, key: key as string, keyInfo: ki, offset: node.content }
		case 'array':
			return { kind: 'array', start: node.left, end: node.right, key: key as string, keyInfo: ki, offset: node.content }
		case 'chain': {
			let resolvedValue: string | undefined
			if (context) {
				try {
					const resolved = resolve(context, node)
					if (typeof resolved === 'string') resolvedValue = resolved
				} catch { }
			}
			return { kind: 'pathChain', start: node.left, end: node.right, key: key as string, keyInfo: ki, offset: node.content, resolvedValue }
		}
	}
}

export const realParser: RexcParser = {
	parseRoot(input) {
		const context = makeContext(input)
		return rxNodeToRexcNode(get(input, input.length), undefined, undefined, context)
	},
	parseChildren(input, parent): RexcNode[] {
		const context = makeContext(input)
		if (parent.kind === 'object') {
			const node = get(input, parent.end) as RxObject
			return [...getEntries(context, node)].map(([keyNode, child]) => {
				const key = resolve(context, keyNode)
				const ki = keyInfoFromRxNode(keyNode, context)
				return rxNodeToRexcNode(child, typeof key === 'string' ? key : String(key), ki, context)
			})
		}
		if (parent.kind === 'array') {
			const node = get(input, parent.end) as RxArray
			return [...getEach(context, node)].map((child, i) => rxNodeToRexcNode(child, i, undefined, context))
		}
		if (parent.kind === 'pathChain') {
			const node = get(input, parent.end) as RxChain
			return [...getEach(context, node)].map((child, i) => rxNodeToRexcNode(child, i, undefined, context))
		}
		if (parent.kind === 'pointer') {
			if (parent.resolvedKind === 'object' || parent.resolvedKind === 'array') {
				// Follow pointers to the ultimate container and return its children
				const ultimate = followTarget(input, parent.targetOffset)
				if (ultimate.type === 'object') {
					return [...getEntries(context, ultimate as RxObject)].map(([keyNode, child]) => {
						const key = resolve(context, keyNode)
						const ki = keyInfoFromRxNode(keyNode, context)
						return rxNodeToRexcNode(child, typeof key === 'string' ? key : String(key), ki, context)
					})
				}
				if (ultimate.type === 'array') {
					return [...getEach(context, ultimate as RxArray)].map((child, i) => rxNodeToRexcNode(child, i, undefined, context))
				}
			}
			const target = parent.targetOffset
			if (target < input.length) return [rxNodeToRexcNode(get(input, target), undefined, undefined, context)]
		}
		return []
	},
	parseKeyNode(input, keyInfo) {
		if (keyInfo.kind === 'plain') return null
		const context = makeContext(input)
		const node = get(input, keyInfo.offset)
		return rxNodeToRexcNode(node, undefined, undefined, context)
	}
}
