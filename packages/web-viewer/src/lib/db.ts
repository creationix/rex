/**
 * IndexedDB storage for REXC documents.
 * Simple key-value store — each document is a row in the "documents" object store.
 */

export interface DocRecord {
	id: string
	name: string
	contentHash: string
	rexcText: string
	jsonText: string
	refsText: string
	refsEnabled: boolean
	mode: string
	updatedAt: number
}

const DB_NAME = 'rexc-viewer'
const DB_VERSION = 1
const STORE = 'documents'

function open(): Promise<IDBDatabase> {
	return new Promise((resolve, reject) => {
		const req = indexedDB.open(DB_NAME, DB_VERSION)
		req.onupgradeneeded = () => {
			const db = req.result
			if (!db.objectStoreNames.contains(STORE)) {
				db.createObjectStore(STORE, { keyPath: 'id' })
			}
		}
		req.onsuccess = () => resolve(req.result)
		req.onerror = () => reject(req.error)
	})
}

export async function listDocs(): Promise<DocRecord[]> {
	const db = await open()
	return new Promise((resolve, reject) => {
		const tx = db.transaction(STORE, 'readonly')
		const store = tx.objectStore(STORE)
		const req = store.getAll()
		req.onsuccess = () => {
			const docs = req.result as DocRecord[]
			docs.sort((a, b) => b.updatedAt - a.updatedAt)
			resolve(docs)
		}
		req.onerror = () => reject(req.error)
	})
}

export async function getDoc(id: string): Promise<DocRecord | undefined> {
	const db = await open()
	return new Promise((resolve, reject) => {
		const tx = db.transaction(STORE, 'readonly')
		const req = tx.objectStore(STORE).get(id)
		req.onsuccess = () => resolve(req.result as DocRecord | undefined)
		req.onerror = () => reject(req.error)
	})
}

export async function putDoc(doc: DocRecord): Promise<void> {
	const db = await open()
	return new Promise((resolve, reject) => {
		const tx = db.transaction(STORE, 'readwrite')
		tx.objectStore(STORE).put(doc)
		tx.oncomplete = () => resolve()
		tx.onerror = () => reject(tx.error)
	})
}

export async function deleteDoc(id: string): Promise<void> {
	const db = await open()
	return new Promise((resolve, reject) => {
		const tx = db.transaction(STORE, 'readwrite')
		tx.objectStore(STORE).delete(id)
		tx.oncomplete = () => resolve()
		tx.onerror = () => reject(tx.error)
	})
}
