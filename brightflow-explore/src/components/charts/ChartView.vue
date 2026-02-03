<script setup>
import { computed, ref, watch } from 'vue'
import { use } from 'echarts/core'
import { CanvasRenderer } from 'echarts/renderers'
import { BarChart, LineChart, PieChart, ScatterChart } from 'echarts/charts'
import {
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  GridComponent
} from 'echarts/components'
import VChart from 'vue-echarts'
import { useResultsStore } from '@/stores/results'
import { useUiStore } from '@/stores/ui'
import { usePivotStore } from '@/stores/pivot'

// Register ECharts components
use([
  CanvasRenderer,
  BarChart,
  LineChart,
  PieChart,
  ScatterChart,
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  GridComponent
])

const resultsStore = useResultsStore()
const uiStore = useUiStore()
const pivotStore = usePivotStore()

// Use pivot data when available, otherwise table data
const chartColumns = computed(() => {
  if (pivotStore.isConfigured && resultsStore.hasPivotResults) {
    return resultsStore.pivotColumns
  }
  return resultsStore.tableColumns
})

const chartRows = computed(() => {
  if (pivotStore.isConfigured && resultsStore.hasPivotResults) {
    return resultsStore.pivotRows
  }
  return resultsStore.tableRows
})

const hasChartData = computed(() => {
  if (pivotStore.isConfigured && resultsStore.hasPivotResults) {
    return true
  }
  return resultsStore.hasTableResults
})

const chartTypes = [
  { label: 'Bar', value: 'bar' },
  { label: 'Line', value: 'line' },
  { label: 'Pie', value: 'pie' },
  { label: 'Scatter', value: 'scatter' }
]

const stackOptions = [
  { label: 'None', value: 'none' },
  { label: 'Stacked', value: 'stacked' },
  { label: '100%', value: 'percent' }
]

// Chart settings
const stacking = ref('none')
const showValues = ref(false)
const horizontal = ref(false)

// Find suitable columns for chart (from pivot or table data)
const stringColumns = computed(() =>
  chartColumns.value.filter(c => ['string', 'text', 'varchar'].includes(c.dtype)).map(c => c.name)
)

const numericColumns = computed(() =>
  chartColumns.value.filter(c => ['int', 'float', 'decimal', 'number', 'i64', 'f64'].includes(c.dtype)).map(c => c.name)
)

const allColumnNames = computed(() =>
  chartColumns.value.map(c => c.name)
)

// Chart axis selections
const xAxis = ref(null)
const yAxes = ref([]) // Support multiple Y axes

// Detect if this is pivot data with column breakdown (multiple value columns)
const isPivotWithColumns = computed(() => {
  return pivotStore.isConfigured &&
         pivotStore.columnFields.length > 0 &&
         resultsStore.hasPivotResults
})

// Get the index/row columns from pivot (these should be X axis candidates)
const pivotIndexColumns = computed(() => {
  if (!isPivotWithColumns.value) return []
  return pivotStore.rowFields.map(f => f.column)
})

// Auto-select axes when data changes
watch(
  () => [stringColumns.value, numericColumns.value, isPivotWithColumns.value, chartColumns.value],
  () => {
    // For pivot with columns, auto-select index column as X and all numeric as Y
    if (isPivotWithColumns.value && chartColumns.value.length > 0) {
      // X axis: first index column (row field)
      if (pivotIndexColumns.value.length > 0) {
        xAxis.value = pivotIndexColumns.value[0]
      } else if (stringColumns.value.length > 0) {
        xAxis.value = stringColumns.value[0]
      }
      // Y axes: all numeric columns (the pivoted values)
      if (numericColumns.value.length > 0) {
        yAxes.value = [...numericColumns.value]
        // Auto-enable stacking for pivot data
        if (stacking.value === 'none' && numericColumns.value.length > 1) {
          stacking.value = 'stacked'
        }
      }
    } else {
      // Regular data - select first of each
      if (stringColumns.value.length > 0 && !xAxis.value) {
        xAxis.value = stringColumns.value[0]
      }
      if (numericColumns.value.length > 0 && yAxes.value.length === 0) {
        yAxes.value = [numericColumns.value[0]]
      }
    }
  },
  { immediate: true }
)

// Select all numeric columns as Y axes
function selectAllYAxes() {
  yAxes.value = [...numericColumns.value]
}

// Check if all numeric columns are selected
const allYAxesSelected = computed(() =>
  numericColumns.value.length > 0 &&
  numericColumns.value.every(c => yAxes.value.includes(c))
)

// Color palette
const colors = [
  '#5470c6', '#91cc75', '#fac858', '#ee6666', '#73c0de',
  '#3ba272', '#fc8452', '#9a60b4', '#ea7ccc'
]

// Build ECharts options
const chartOption = computed(() => {
  if (!xAxis.value || yAxes.value.length === 0 || !hasChartData.value) {
    return null
  }

  const xIndex = chartColumns.value.findIndex(c => c.name === xAxis.value)
  if (xIndex === -1) return null

  const yIndices = yAxes.value.map(y => chartColumns.value.findIndex(c => c.name === y))
  if (yIndices.some(i => i === -1)) return null

  const xData = chartRows.value.map(row => row[xIndex])

  const baseOption = {
    color: colors,
    tooltip: {
      trigger: 'axis',
      axisPointer: { type: 'shadow' }
    },
    legend: yAxes.value.length > 1 ? {
      data: yAxes.value,
      bottom: 0
    } : undefined,
    grid: {
      left: '3%',
      right: '4%',
      bottom: yAxes.value.length > 1 ? '10%' : '3%',
      containLabel: true
    }
  }

  // Build series for each Y axis
  const buildSeries = (type) => {
    return yIndices.map((yIdx, i) => {
      const yData = chartRows.value.map(row => row[yIdx])
      const series = {
        name: yAxes.value[i],
        type,
        data: yData,
        itemStyle: { color: colors[i % colors.length] }
      }

      // Stacking for bar/line
      if (stacking.value !== 'none' && (type === 'bar' || type === 'line')) {
        series.stack = 'total'
        if (stacking.value === 'percent') {
          series.stackStrategy = 'all'
        }
      }

      // Show values on data points
      if (showValues.value) {
        series.label = {
          show: true,
          position: type === 'bar' ? 'top' : 'top',
          fontSize: 10
        }
      }

      // Area style for line charts
      if (type === 'line') {
        series.smooth = true
        if (stacking.value !== 'none') {
          series.areaStyle = {}
        }
      }

      return series
    })
  }

  switch (uiStore.chartType) {
    case 'bar':
      if (horizontal.value) {
        return {
          ...baseOption,
          xAxis: { type: 'value' },
          yAxis: {
            type: 'category',
            data: xData,
            axisLabel: { width: 100, overflow: 'truncate' }
          },
          series: buildSeries('bar')
        }
      }
      return {
        ...baseOption,
        xAxis: {
          type: 'category',
          data: xData,
          axisLabel: { rotate: xData.length > 10 ? 45 : 0 }
        },
        yAxis: { type: 'value' },
        series: buildSeries('bar')
      }

    case 'line':
      return {
        ...baseOption,
        xAxis: {
          type: 'category',
          data: xData,
          boundaryGap: false
        },
        yAxis: { type: 'value' },
        series: buildSeries('line')
      }

    case 'pie':
      const yData = chartRows.value.map(row => row[yIndices[0]])
      return {
        color: colors,
        tooltip: { trigger: 'item', formatter: '{b}: {c} ({d}%)' },
        legend: { orient: 'vertical', left: 'left' },
        series: [{
          type: 'pie',
          radius: ['40%', '70%'],
          data: xData.map((name, i) => ({
            name: String(name),
            value: yData[i]
          })),
          label: showValues.value ? {
            show: true,
            formatter: '{b}: {d}%'
          } : { show: false }
        }]
      }

    case 'scatter':
      return {
        ...baseOption,
        xAxis: { type: 'value', name: xAxis.value },
        yAxis: { type: 'value', name: yAxes.value[0] },
        series: [{
          type: 'scatter',
          data: chartRows.value.map(row => [row[xIndex], row[yIndices[0]]]),
          itemStyle: { color: colors[0] }
        }]
      }

    default:
      return null
  }
})

const canShowChart = computed(() =>
  hasChartData.value && numericColumns.value.length > 0
)

// Show stacking options only for bar/line
const showStackingOptions = computed(() =>
  ['bar', 'line'].includes(uiStore.chartType)
)

// Show horizontal option only for bar
const showHorizontalOption = computed(() =>
  uiStore.chartType === 'bar'
)
</script>

<template>
  <div class="flex flex-col h-full p-4">
    <!-- Chart config -->
    <div class="flex items-center gap-4 mb-4 flex-wrap">
      <!-- Chart type -->
      <div class="flex items-center gap-2">
        <label class="text-xs text-muted">Type:</label>
        <USelectMenu
          v-model="uiStore.chartType"
          :items="chartTypes"
          value-key="value"
          class="w-24"
          size="xs"
        />
      </div>

      <!-- X Axis -->
      <div class="flex items-center gap-2">
        <label class="text-xs text-muted">X:</label>
        <USelectMenu
          v-model="xAxis"
          :items="stringColumns.length ? stringColumns : allColumnNames"
          placeholder="X axis"
          class="w-32"
          size="xs"
        />
      </div>

      <!-- Y Axes (multi-select) -->
      <div class="flex items-center gap-2">
        <label class="text-xs text-muted">Y:</label>
        <USelectMenu
          v-model="yAxes"
          :items="numericColumns"
          placeholder="Y axis"
          multiple
          class="w-40"
          size="xs"
        />
        <button
          v-if="numericColumns.length > 1 && !allYAxesSelected"
          class="text-xs text-primary hover:text-primary/80 transition-colors"
          @click="selectAllYAxes"
        >
          All
        </button>
      </div>

      <!-- Stacking (for bar/line) -->
      <div v-if="showStackingOptions" class="flex items-center gap-2">
        <label class="text-xs text-muted">Stack:</label>
        <USelectMenu
          v-model="stacking"
          :items="stackOptions"
          value-key="value"
          class="w-24"
          size="xs"
        />
      </div>

      <!-- Horizontal toggle (for bar) -->
      <label v-if="showHorizontalOption" class="flex items-center gap-1.5 text-xs text-muted cursor-pointer">
        <USwitch v-model="horizontal" size="xs" />
        Horizontal
      </label>

      <!-- Show values toggle -->
      <label class="flex items-center gap-1.5 text-xs text-muted cursor-pointer">
        <USwitch v-model="showValues" size="xs" />
        Values
      </label>
    </div>

    <!-- Chart -->
    <div class="flex-1 min-h-0">
      <VChart
        v-if="chartOption"
        :option="chartOption"
        autoresize
        class="w-full h-full"
      />

      <div v-else-if="!canShowChart" class="flex items-center justify-center h-full text-muted">
        <div class="text-center">
          <div class="mb-2">Cannot display chart</div>
          <div class="text-sm text-muted/70">
            Requires at least one numeric column for Y axis
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
