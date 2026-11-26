// Native menu/dialog strings. UI locale is an app preference, never a document property.
const ko = {
  userSettings: 'Folio 사용자 설정', importSettings: '사용자 설정 가져오기', exportSettings: '사용자 설정 내보내기', invalidSettings: '올바른 Folio 사용자 설정 파일이 아닙니다',
  about: 'Folio Studio 정보', services: '서비스', hide: 'Folio Studio 가리기',
  hideOthers: '기타 가리기', unhide: '모두 표시', quit: 'Folio Studio 종료',
  window: '창', minimize: '최소화', zoom: '확대/축소', front: '모두 앞으로 가져오기',
  help: '도움말', logs: '진단 로그 폴더 열기',
  open: '열기 — Folio 문서 또는 가져올 InDesign 문서',
  idml: 'IDML로 내보내기', openInDesign: 'InDesign 문서 열기',
  inDesignDocument: 'InDesign 문서', pdf: 'PDF로 내보내기', linksFolder: '링크를 찾을 폴더',
  place: '가져와 놓기', pictures: 'JPEG · PNG · TIFF · PSD · GIF · BMP · SVG 그림', placeable: '가져올 수 있는 파일', placeText: '글 파일 (TXT · DOCX · RTF)', allFiles: '모든 파일 (지원하지 않는 형식은 이유를 알려 줍니다)',
  saveReport: '보고서 저장', loadSwatches: '색상 견본 불러오기', saveSwatches: '색상 견본 저장', swatchFile: '색상 견본 교환 파일 (ASE)', textFile: '텍스트 파일', saveFolio: 'Folio 문서 저장', folioDocument: 'Folio 문서',
  importPdfComments: 'PDF 주석 가져오기', importComments: '주석 가져오기', pdfFiles: 'PDF 파일',
  engineStartError: '엔진을 시작하지 못했습니다.',
  engineExecutableMissing: '엔진 실행 파일을 찾을 수 없습니다.',
  engineSystemFileError: '필요한 시스템 파일이나 기능을 찾을 수 없어 엔진을 시작하지 못했습니다.',
  engineExitCode: '종료 코드: {status}', engineExitSignal: '종료 신호: {status}',
};
const en = {
  userSettings: 'Folio User Settings', importSettings: 'Import User Settings', exportSettings: 'Export User Settings', invalidSettings: 'Not a valid Folio user settings file',
  about: 'About Folio Studio', services: 'Services', hide: 'Hide Folio Studio',
  hideOthers: 'Hide Others', unhide: 'Show All', quit: 'Quit Folio Studio',
  window: 'Window', minimize: 'Minimize', zoom: 'Zoom', front: 'Bring All to Front',
  help: 'Help', logs: 'Open Diagnostic Logs Folder',
  open: 'Open — Folio or InDesign Document',
  idml: 'Export IDML', openInDesign: 'Open InDesign Document',
  inDesignDocument: 'InDesign Document', pdf: 'Export PDF', linksFolder: 'Link Search Folder',
  place: 'Place', pictures: 'JPEG · PNG · TIFF · PSD · GIF · BMP · SVG Images', placeable: 'Placeable Files', placeText: 'Text Files (TXT · DOCX · RTF)', allFiles: 'All Files (formats that are not supported say why)',
  saveReport: 'Save Report', loadSwatches: 'Load Swatches', saveSwatches: 'Save Swatches', swatchFile: 'Swatch Exchange File (ASE)', textFile: 'Text File', saveFolio: 'Save Folio Document', folioDocument: 'Folio Document',
  importPdfComments: 'Import PDF comments', importComments: 'Import comments', pdfFiles: 'PDF Files',
  engineStartError: 'The engine could not start.',
  engineExecutableMissing: 'The engine executable could not be found.',
  engineSystemFileError: 'A required system file or function could not be found, so the engine could not start.',
  engineExitCode: 'Exit code: {status}', engineExitSignal: 'Exit signal: {status}',
};
const language = (choice, os) => choice === 'ko' || choice === 'en' ? choice : /^ko(?:-|$)/i.test(os || '') ? 'ko' : 'en';
const translator = (choice, os) => { const table = language(choice, os) === 'ko' ? ko : en; return key => table[key] ?? key; };
module.exports = { translator, language, ko, en };
