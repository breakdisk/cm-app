package io.logisticos.driver.core.database.di

import android.content.Context
import androidx.room.Room
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.logisticos.driver.core.database.DriverDatabase
import io.logisticos.driver.core.database.dao.*
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object DatabaseModule {

    @Provides @Singleton
    fun provideDatabase(@ApplicationContext context: Context): DriverDatabase =
        Room.databaseBuilder(context, DriverDatabase::class.java, "driver_app.db")
            .fallbackToDestructiveMigration()
            .build()

    @Provides fun provideShiftDao(db: DriverDatabase): ShiftDao = db.shiftDao()
    @Provides fun provideTaskDao(db: DriverDatabase): TaskDao = db.taskDao()
    @Provides fun provideRouteDao(db: DriverDatabase): RouteDao = db.routeDao()
    @Provides fun providePodDao(db: DriverDatabase): PodDao = db.podDao()
    @Provides fun provideLocationBreadcrumbDao(db: DriverDatabase): LocationBreadcrumbDao = db.locationBreadcrumbDao()
    @Provides fun provideScanEventDao(db: DriverDatabase): ScanEventDao = db.scanEventDao()
    @Provides fun provideSyncQueueDao(db: DriverDatabase): SyncQueueDao = db.syncQueueDao()
}
