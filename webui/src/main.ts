import { mount } from 'svelte';
import '@fontsource/barlow/400.css';
import '@fontsource/barlow/500.css';
import '@fontsource/barlow/600.css';
import '@fontsource/barlow-condensed/500.css';
import '@fontsource/barlow-condensed/600.css';
import '@fontsource/rajdhani/400.css';
import './app.css';
import App from './App.svelte';

mount(App, { target: document.getElementById('app')! });
