/**
 * Single-import ECharts component registration for the Topics view.
 * Imported for side-effect by TopicsPie.
 */
import { PieChart } from 'echarts/charts';
import { LegendComponent, TitleComponent, TooltipComponent } from 'echarts/components';
import { use } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';

use([CanvasRenderer, PieChart, TitleComponent, TooltipComponent, LegendComponent]);
