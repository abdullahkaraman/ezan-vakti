const question = document.getElementById("question");

window.setQuestion = (text) => {
  question.textContent = text;
};
window.setQuestion(window.__QUESTION__ ?? "");

// Closing must not depend on the log: the answer is sent, then the window goes.
const answer = async (value) => {
  try {
    await window.__TAURI__.core.invoke("answer_prayer_check", { answer: value });
  } catch (_) {}
  window.__TAURI__.window.getCurrentWindow().close();
};
document.getElementById("yes").addEventListener("click", () => answer("kildim"));
document.getElementById("no").addEventListener("click", () => answer("henuz_degil"));
