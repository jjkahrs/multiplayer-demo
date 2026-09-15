using UnityEngine;
using UnityEngine.UI;

namespace Demo
{
    public class JoinScreen : MonoBehaviour
    {
        private const string DefaultHost = "ws://127.0.0.1:8080/ws";
        private const int MaxNameLength = 16;

        [SerializeField] private NetworkClient client;
        [SerializeField] private GameObject panel;
        [SerializeField] private InputField hostField;
        [SerializeField] private InputField nameField;
        [SerializeField] private Button joinButton;
        [SerializeField] private Text statusText;

        private void Awake()
        {
            nameField.characterLimit = MaxNameLength;
            if (string.IsNullOrEmpty(hostField.text)) hostField.text = DefaultHost;
            joinButton.onClick.AddListener(OnJoinClicked);
            ShowState(client.CurrentState);
        }

        // Standalone build joins without typing: -host <url> -name <name>.
        private void Start()
        {
            var args = System.Environment.GetCommandLineArgs();
            var host = ArgValue(args, "-host");
            if (host != null) hostField.text = host;
            var name = ArgValue(args, "-name");
            if (name == null) return;
            nameField.text = name;
            OnJoinClicked();
        }

        public static string ArgValue(string[] args, string key)
        {
            int i = System.Array.IndexOf(args, key);
            return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
        }

        private void OnEnable()
        {
            client.OnStateChanged += ShowState;
            client.OnError += ShowError;
            client.OnDisconnected += ShowDisconnected;
        }

        private void OnDisable()
        {
            client.OnStateChanged -= ShowState;
            client.OnError -= ShowError;
            client.OnDisconnected -= ShowDisconnected;
        }

        private void OnJoinClicked()
        {
            client.Connect(hostField.text.Trim());
            client.Join(nameField.text);
        }

        private void ShowState(NetworkClient.State state)
        {
            statusText.text = state.ToString();
            panel.SetActive(state != NetworkClient.State.InWorld);
            statusText.gameObject.SetActive(state != NetworkClient.State.InWorld); // the HUD owns top-left in world
            joinButton.interactable = state == NetworkClient.State.Disconnected || state == NetworkClient.State.Joining;
        }

        private void ShowError(ServerErrorMsg error) => statusText.text = $"Error: {error.message}";

        private void ShowDisconnected(string reason) =>
            statusText.text = reason == null ? "Disconnected" : $"Disconnected: {reason}";
    }
}
