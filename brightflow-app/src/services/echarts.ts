/**
 * Single-import ECharts registration for the whole app.
 *
 * Every chart surface (explore charts, dashboards, insight renderers, topics)
 * imports this module for its side effect. One union registration instead of
 * four per-area lists: the areas ship in the same bundle anyway, and split
 * lists kept drifting (a renderer adding a mark type had to know which copy
 * to touch).
 */

import { BarChart, CustomChart, LineChart, PieChart, ScatterChart } from 'echarts/charts';
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  MarkPointComponent,
  TitleComponent,
  TooltipComponent,
} from 'echarts/components';
import { use } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';

use([
  CanvasRenderer,
  BarChart,
  LineChart,
  PieChart,
  ScatterChart,
  CustomChart,
  GridComponent,
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  DataZoomComponent,
  MarkAreaComponent,
  MarkLineComponent,
  MarkPointComponent,
]);
