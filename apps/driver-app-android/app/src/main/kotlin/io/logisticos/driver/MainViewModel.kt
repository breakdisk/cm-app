package io.logisticos.driver

import androidx.lifecycle.ViewModel
import com.google.firebase.messaging.FirebaseMessaging
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.feature.auth.data.AuthRepository
import io.logisticos.driver.feature.notifications.data.NotificationRepository
import javax.inject.Inject

@HiltViewModel
class MainViewModel @Inject constructor(
    private val authRepository: AuthRepository,
    private val notificationRepository: NotificationRepository
) : ViewModel() {

    val isLoggedIn: Boolean get() = authRepository.isLoggedIn()

    // Called from AppNavGraph immediately after OTP verification succeeds.
    // FCM's onNewToken only fires on token rotation — if the token was issued
    // before the driver logged in, we must explicitly fetch and register it now.
    //
    // Guarded with runCatching: FirebaseApp.initializeApp() is driven by the
    // google-services plugin which requires google-services.json at build time.
    // Debug builds that omit the file (or builds where the plugin is disabled)
    // will throw IllegalStateException from getInstance(). Push notifications
    // won't work in that case but all other driver operations are unaffected.
    fun onAuthSuccess() {
        runCatching {
            FirebaseMessaging.getInstance().token.addOnSuccessListener { fcmToken ->
                notificationRepository.registerFcmToken(fcmToken)
            }
        }
    }
}
