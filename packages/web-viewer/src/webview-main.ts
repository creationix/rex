import { mount } from 'svelte'
import WebviewApp from './WebviewApp.svelte'
import './app.css'

mount(WebviewApp, { target: document.getElementById('app')! })
