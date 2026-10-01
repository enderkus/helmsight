import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import 'uplot/dist/uPlot.min.css';
import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { PRODUCT_NAME } from './lib/brand';

document.title = PRODUCT_NAME;

const target = document.getElementById('app');
if (target) mount(App, { target });
