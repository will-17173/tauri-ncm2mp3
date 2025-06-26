<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

const isDragOver = ref(false);
const isConverting = ref(false);
const progress = ref({
  total: 0,
  processed: 0,
  currentFile: "",
  status: ""
});
const results = ref([]);
const logs = ref([]);

let unlistenProgress = null;
let unlistenFileDrop = null;

onMounted(async () => {
  unlistenProgress = await listen("conversion-progress", (event) => {
    progress.value = event.payload;
  });
  
  try {
    // Tauri 2.0 文件拖拽事件监听
    const window = getCurrentWindow();
    
    // 监听文件拖拽事件 - 尝试多种可能的事件名称
    const dragEvents = [
      "tauri://file-drop",
      "tauri://drag-drop", 
      "file-drop",
      "drag-drop"
    ];
    
    const hoverEvents = [
      "tauri://file-drop-hover",
      "tauri://drag-hover",
      "file-drop-hover", 
      "drag-hover"
    ];
    
    const leaveEvents = [
      "tauri://file-drop-cancelled",
      "tauri://drag-leave",
      "file-drop-cancelled",
      "drag-leave"
    ];
    
    // 尝试监听所有可能的拖拽事件
    for (const eventName of dragEvents) {
      try {
        await window.listen(eventName, (event) => {
          console.log(`拖拽事件 ${eventName}:`, event);
          console.log(`事件载荷详情:`, JSON.stringify(event.payload, null, 2));
          let files = [];
          
          if (event.payload) {
            if (Array.isArray(event.payload)) {
              files = event.payload;
            } else if (event.payload.paths) {
              files = event.payload.paths;
            } else if (event.payload.files) {
              files = event.payload.files;
            }
          }
          
          // 详细调试每个文件路径
          files.forEach((file, index) => {
            console.log(`文件 ${index}:`, file);
            console.log(`文件 ${index} 类型:`, typeof file);
            console.log(`文件 ${index} 长度:`, file.length);
            console.log(`文件 ${index} 字符码:`, [...file].map(c => c.charCodeAt(0)));
          });
          
          if (files.length > 0) {
            addLog(`收到拖拽文件: ${files.length} 个`);
            convertFiles(files);
          }
          isDragOver.value = false;
        });
        console.log(`成功监听事件: ${eventName}`);
      } catch (e) {
        console.log(`监听事件 ${eventName} 失败:`, e);
      }
    }
    
    // 监听悬停事件
    for (const eventName of hoverEvents) {
      try {
        await window.listen(eventName, (event) => {
          console.log(`拖拽悬停事件 ${eventName}:`, event);
          isDragOver.value = true;
        });
      } catch (e) {
        console.log(`监听悬停事件 ${eventName} 失败:`, e);
      }
    }
    
    // 监听离开事件
    for (const eventName of leaveEvents) {
      try {
        await window.listen(eventName, (event) => {
          console.log(`拖拽离开事件 ${eventName}:`, event);
          isDragOver.value = false;
        });
      } catch (e) {
        console.log(`监听离开事件 ${eventName} 失败:`, e);
      }
    }
    
    // addLog("拖拽功能初始化完成", 'info'); 
    
  } catch (error) {
    console.error("设置拖拽监听失败:", error);
    addLog(`拖拽功能初始化失败: ${error}`, 'error');
  }
});

onUnmounted(() => {
  if (unlistenProgress) {
    unlistenProgress();
  }
  if (unlistenFileDrop) {
    unlistenFileDrop();
  }
});

// 严格的路径验证函数 - 只允许安全的ASCII路径
function isValidPath(path) {
  try {
    if (typeof path !== 'string' || path.length === 0) {
      return false;
    }
    
    // 检查是否包含乱码字符或替换字符
    if (path.includes('�') || /[\u{FFFD}\u{FFF0}-\u{FFFF}]/u.test(path)) {
      console.warn('路径包含乱码字符:', path);
      return false;
    }
    
    // 检查是否包含明显的编码错误产生的字符
    const invalidChars = ['☒', '☐', '맊', '망', '○', '瘟', '擦', '霆', '潞', '掘', '昁', '悝', '椆', '恊', '印', '獰', '激', '城', '呂', '鳥'];
    for (const char of invalidChars) {
      if (path.includes(char)) {
        console.warn('路径包含无效字符:', char, 'in', path);
        return false;
      }
    }
    
    // 检查是否包含过多的非ASCII字符（可能是编码错误）
    const nonAsciiCount = (path.match(/[^\x00-\x7F]/g) || []).length;
    const totalLength = path.length;
    if (nonAsciiCount > totalLength * 0.3) { // 如果超过30%是非ASCII字符，可能有问题
      console.warn('路径包含过多非ASCII字符，可能存在编码问题:', path);
      return false;
    }
    
    return true;
  } catch (error) {
    console.error('路径验证出错:', error);
    return false;
  }
}

// 安全的路径显示函数，避免乱码
function safePathDisplay(path) {
  // 直接返回友好提示，不尝试显示可能损坏的路径
  return '[文件路径编码异常]';
}

function addLog(message, type = 'info') {
  logs.value.push({
    id: Date.now(),
    message,
    type,
    timestamp: new Date().toLocaleTimeString()
  });
}

async function selectFiles() {
  try {
    const selected = await open({
      multiple: true,
      filters: [{
        name: 'NCM Files',
        extensions: ['ncm']
      }]
    });
    
    if (selected) {
      const files = Array.isArray(selected) ? selected : [selected];
      await convertFiles(files);
    }
  } catch (error) {
    addLog(`选择文件失败: ${error}`, 'error');
  }
}

async function selectFolder() {
  try {
    const selected = await open({
      directory: true
    });
    
    if (selected) {
      await convertFolder(selected);
    }
  } catch (error) {
    addLog(`选择文件夹失败: ${error}`, 'error');
  }
}

async function convertFiles(filePaths) {
  isConverting.value = true;
  results.value = [];
  addLog(`收到拖拽文件: ${filePaths.length} 个`);
  
  // 过滤和修复文件路径
  const validPaths = [];
  const invalidPaths = [];
  
  filePaths.forEach(path => {
    console.log('处理路径:', path, '类型:', typeof path);
    if (isValidPath(path)) {
      validPaths.push(path);
    } else {
      invalidPaths.push(path);
      addLog(`跳过路径编码异常的文件`, 'error');
    }
  });
  
  if (invalidPaths.length > 0) {
    addLog(`检测到 ${invalidPaths.length} 个文件存在路径编码问题，已跳过`, 'error');
    addLog('⚠️ Windows拖拽编码问题解决方案：', 'info');
    addLog('1. 将文件移动到英文路径下（如 C:\\temp\\）', 'info');
    addLog('2. 重命名文件为英文名称', 'info');
    addLog('3. 或者使用下方"选择文件"按钮', 'info');
  }
  
  if (validPaths.length === 0) {
    addLog('❌ 所有拖拽的文件都存在路径编码问题', 'error');
    addLog('🔧 建议使用"选择文件"或"选择文件夹"按钮，这些功能不受编码问题影响', 'info');
    isConverting.value = false;
    return;
  }
  
  addLog(`将处理 ${validPaths.length} 个有效文件路径`);
  
  try {
    // 先检查哪些是文件，哪些是文件夹
    const allNcmFiles = [];
    
    for (let filePath of validPaths) {
      try {
        // 检查路径类型
        const isDirectory = await invoke("is_directory", { path: filePath });
        
        if (isDirectory) {
          addLog(`正在扫描文件夹...`);
          // 如果是文件夹，递归查找NCM文件
          const ncmFiles = await invoke("find_ncm_files", { folderPath: filePath });
          allNcmFiles.push(...ncmFiles);
          addLog(`在文件夹中找到 ${ncmFiles.length} 个NCM文件`);
        } else {
          // 如果是文件，检查是否为NCM文件
          if (filePath.toLowerCase().endsWith('.ncm')) {
            allNcmFiles.push(filePath);
          } else {
            addLog(`跳过非NCM文件`, 'error');
          }
        }
      } catch (error) {
        addLog(`处理文件路径时出错: ${error}`, 'error');
      }
    }
    
    if (allNcmFiles.length === 0) {
      addLog('没有找到NCM文件', 'error');
      return;
    }
    
    addLog(`开始转换 ${allNcmFiles.length} 个NCM文件`);
    
    // 转换所有找到的NCM文件
    for (let filePath of allNcmFiles) {
      addLog(`正在转换NCM文件...`);
      const result = await invoke("convert_ncm_file", { filePath });
      results.value.push(result);
      
      if (result.success) {
        addLog(`转换成功: ${result.output_path || '输出路径未知'}`, 'success');
      } else {
        addLog(`转换失败: ${result.message}`, 'error');
      }
    }
  } catch (error) {
    addLog(`转换过程出错: ${error}`, 'error');
  } finally {
    isConverting.value = false;
    addLog('转换完成');
  }
}

async function convertFolder(folderPath) {
  isConverting.value = true;
  results.value = [];
  addLog(`开始转换文件夹: ${folderPath}`);
  
  try {
    const folderResults = await invoke("convert_ncm_folder", { folderPath });
    results.value = folderResults;
    
    const successCount = folderResults.filter(r => r.success).length;
    const failCount = folderResults.length - successCount;
    
    addLog(`文件夹转换完成: 成功 ${successCount} 个，失败 ${failCount} 个`, 'success');
  } catch (error) {
    addLog(`文件夹转换失败: ${error}`, 'error');
  } finally {
    isConverting.value = false;
  }
}

async function handleDrop(e) {
  e.preventDefault();
  e.stopPropagation();
  isDragOver.value = false;
  
  console.log("HTML拖拽事件:", e);
  
  // 处理HTML5拖拽API
  if (e.dataTransfer && e.dataTransfer.files) {
    const files = Array.from(e.dataTransfer.files);
    console.log("拖拽的文件:", files);
    
    // 在Tauri中，需要获取文件路径
    const filePaths = [];
    for (const file of files) {
      // Tauri环境下文件对象应该有path属性
      if (file.path) {
        filePaths.push(file.path);
      } else if (file.name) {
        // 备用方案：尝试使用文件名
        addLog(`警告: 无法获取文件路径，仅获得文件名: ${file.name}`, 'error');
      }
    }
    
    if (filePaths.length > 0) {
      addLog(`HTML拖拽收到文件: ${filePaths.length} 个`);
      convertFiles(filePaths);
    } else {
      addLog("拖拽的文件无法获取路径", 'error');
    }
  }
}

function handleDragOver(e) {
  e.preventDefault();
  e.stopPropagation();
  isDragOver.value = true;
}

function handleDragLeave(e) {
  e.preventDefault();
  e.stopPropagation();
  isDragOver.value = false;
}

function clearLogs() {
  logs.value = [];
}

async function openWebsite() {
  try {
    // Tauri 2.0 正确的调用方式
    await invoke('plugin:opener|open_url', { 
      url: 'https://ncm2mp3.xyz/' 
    });
  } catch (error) {
    addLog(`打开官网失败: ${error}`, 'error');
  }
}

async function openGithub() {
  try {
    // Tauri 2.0 正确的调用方式
    await invoke('plugin:opener|open_url', { 
      url: 'https://github.com/will-17173/tauri-ncm2mp3' 
    });
  } catch (error) {
    addLog(`打开GitHub失败: ${error}`, 'error');
  }
}
</script>

<template>
  <main class="container">
    <!-- 右上角链接图标 -->
    <div class="header-links">
      <button @click="openWebsite" class="icon-link" title="官方网站">
        🌐
      </button>
      <button @click="openGithub" class="icon-link" title="GitHub">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
        </svg>
      </button>
    </div>
    
    <h1>NCM to MP3 转换器</h1>

    <div 
      class="drop-zone"
      :class="{ 'drag-over': isDragOver, 'converting': isConverting }"
      @drop="handleDrop"
      @dragover="handleDragOver"
      @dragleave="handleDragLeave"
    >
      <div class="drop-content">
        <div class="drop-icon">📁</div>
        <p v-if="!isConverting">拖拽NCM文件或文件夹到这里</p>
        <p v-else>正在转换中...</p>
        
        <!-- Windows拖拽限制提示 -->
        <div class="windows-notice" v-if="!isConverting">
          <p class="notice-text">⚠️ Windows用户注意：如果拖拽失败，请使用下方按钮</p>
        </div>
        
        <div class="button-group" v-if="!isConverting">
          <button @click="selectFiles" class="select-btn">选择文件</button>
          <button @click="selectFolder" class="select-btn">选择文件夹</button>
        </div>
        
        <div class="progress-info" v-if="isConverting && progress.total > 0">
          <div class="progress-bar">
            <div 
              class="progress-fill"
              :style="{ width: `${(progress.processed / progress.total) * 100}%` }"
            ></div>
          </div>
          <p>{{ progress.processed }} / {{ progress.total }}</p>
          <p class="current-file">{{ progress.currentFile }}</p>
        </div>
      </div>
    </div>

    <div class="logs-section" v-if="logs.length > 0">
      <div class="logs-header">
        <h3>转换日志</h3>
        <button @click="clearLogs" class="clear-btn">清空</button>
      </div>
      <div class="logs-container">
        <div 
          v-for="log in logs" 
          :key="log.id"
          class="log-item"
          :class="log.type"
        >
          <span class="log-time">{{ log.timestamp }}</span>
          <span class="log-message">{{ log.message }}</span>
        </div>
      </div>
    </div>
  </main>
</template>

<style scoped>
.container {
  max-width: 800px;
  margin: 0 auto;
  padding: 2rem;
  position: relative;
}

.header-links {
  position: absolute;
  top: 1rem;
  right: 1rem;
  display: flex;
  gap: 0.5rem;
}

.icon-link {
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 50%;
  background-color: rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.2);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 1.2rem;
  transition: all 0.3s ease;
  color: #4a5568;
}

.icon-link svg {
  width: 20px;
  height: 20px;
}

.icon-link:hover {
  background-color: rgba(255, 255, 255, 0.2);
  transform: scale(1.1);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

h1 {
  text-align: center;
  color: #2c3e50;
  margin-bottom: 2rem;
}

.drop-zone {
  border: 3px dashed #cbd5e0;
  border-radius: 12px;
  padding: 3rem;
  text-align: center;
  transition: all 0.3s ease;
  background-color: #f8fafc;
  min-height: 200px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.drop-zone.drag-over {
  border-color: #4299e1;
  background-color: #ebf8ff;
}

.drop-zone.converting {
  border-color: #38a169;
  background-color: #f0fff4;
}

.drop-content {
  width: 100%;
}

.drop-icon {
  font-size: 4rem;
  margin-bottom: 1rem;
}

.drop-zone p {
  font-size: 1.2rem;
  color: #4a5568;
  margin-bottom: 1.5rem;
}

.windows-notice {
  margin: 1rem 0;
  padding: 0.75rem;
  background-color: #fff3cd;
  border: 1px solid #ffeaa7;
  border-radius: 6px;
  max-width: 400px;
  margin-left: auto;
  margin-right: auto;
}

.notice-text {
  margin: 0;
  font-size: 0.9rem;
  color: #856404;
  text-align: center;
  line-height: 1.4;
}

.button-group {
  display: flex;
  gap: 1rem;
  justify-content: center;
}

.select-btn {
  background-color: #4299e1;
  color: white;
  border: none;
  padding: 0.75rem 1.5rem;
  border-radius: 8px;
  font-size: 1rem;
  cursor: pointer;
  transition: background-color 0.3s ease;
}

.select-btn:hover {
  background-color: #3182ce;
}

.progress-info {
  margin-top: 2rem;
}

.progress-bar {
  width: 100%;
  height: 8px;
  background-color: #e2e8f0;
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 1rem;
}

.progress-fill {
  height: 100%;
  background-color: #38a169;
  transition: width 0.3s ease;
}

.current-file {
  font-size: 0.9rem;
  color: #718096;
  font-style: italic;
}

.logs-section {
  margin-top: 2rem;
}

.logs-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.logs-header h3 {
  color: #2d3748;
  margin: 0;
}

.clear-btn {
  background-color: #e53e3e;
  color: white;
  border: none;
  padding: 0.5rem 1rem;
  border-radius: 6px;
  font-size: 0.9rem;
  cursor: pointer;
}

.clear-btn:hover {
  background-color: #c53030;
}

.logs-container {
  max-height: 300px;
  overflow-y: auto;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  background-color: #ffffff;
}

.log-item {
  padding: 0.75rem;
  border-bottom: 1px solid #f7fafc;
  display: flex;
  gap: 1rem;
}

.log-item:last-child {
  border-bottom: none;
}

.log-item.success {
  background-color: #f0fff4;
  border-left: 4px solid #38a169;
}

.log-item.error {
  background-color: #fed7d7;
  border-left: 4px solid #e53e3e;
}

.log-item.info {
  background-color: #ebf8ff;
  border-left: 4px solid #4299e1;
}

.log-time {
  color: #718096;
  font-size: 0.8rem;
  white-space: nowrap;
  font-family: monospace;
}

.log-message {
  flex: 1;
  font-size: 0.9rem;
}

@media (prefers-color-scheme: dark) {
  h1 {
    color: #f7fafc;
  }
  
  .drop-zone {
    background-color: #2d3748;
    border-color: #4a5568;
  }
  
  .drop-zone.drag-over {
    background-color: #2c5282;
    border-color: #63b3ed;
  }
  
  .drop-zone.converting {
    background-color: #276749;
    border-color: #68d391;
  }
  
  .drop-zone p {
    color: #cbd5e0;
  }
  
  .logs-container {
    background-color: #2d3748;
    border-color: #4a5568;
  }
  
  .log-item {
    border-bottom-color: #4a5568;
  }
  
  .log-item.success {
    background-color: #276749;
  }
  
  .log-item.error {
    background-color: #742a2a;
  }
  
  .log-item.info {
    background-color: #2c5282;
  }
  
  .log-message {
    color: #e2e8f0;
  }
  
  .icon-link {
    background-color: rgba(0, 0, 0, 0.2);
    border-color: rgba(255, 255, 255, 0.1);
    color: #cbd5e0;
  }
  
  .icon-link:hover {
    background-color: rgba(0, 0, 0, 0.3);
  }
  
  .windows-notice {
    background-color: #2d3748;
    border-color: #4a5568;
  }
  
  .notice-text {
    color: #fbd38d;
  }
}
</style>
