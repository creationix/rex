/**
 * Interface for lazily parsing REXC into a tree structure.
 *
 * The parser operates on a Uint8Array (UTF-8 encoded REXC).
 * All offsets (`start`, `end`) are byte indices into that array,
 * matching how REXC length prefixes and pointers work.
 *
 * The parser should NOT recurse into children — only return the immediate
 * children of the requested node.
 */

export type RexcNode =
  { kind: 'object', start: number, end: number, key?: string, offset: number } |
  { kind: 'array', start: number, end: number, key?: string, offset: number } |
  { kind: 'string', start: number, end: number, key?: string, value: string } |
  { kind: 'pathChain', start: number, end: number, key?: string, offset: number, resolvedValue?: string } |
  { kind: 'number', start: number, end: number, key?: string, value: number } |
  { kind: 'boolean', start: number, end: number, key?: string, value: boolean } |
  { kind: 'null', start: number, end: number, key?: string } |
  { kind: 'undefined', start: number, end: number, key?: string } |
  { kind: 'reference', start: number, end: number, key?: string, refId: string } |
  { kind: 'pointer', start: number, end: number, key?: string, targetOffset: number, resolvedValue?: string, resolvedKind?: string }

export interface RexcParser {
  /**
   * Parse the single top-level value from raw REXC bytes.
   * Does NOT recurse — the returned node is shallow.
   */
  parseRoot(input: Uint8Array): RexcNode

  /**
   * Get the direct children of an expandable node.
   * Called lazily when the user expands a node in the tree.
   *
   * For objects: returns one node per value, with `key` set to the string key.
   * For arrays: returns one node per element, with `key` set to the numeric index.
   * For other containers (call, when, etc.): returns operand nodes in order.
   *
   * Does NOT recurse — each returned child is shallow.
   */
  parseChildren(input: Uint8Array, parent: RexcNode): RexcNode[]
}
